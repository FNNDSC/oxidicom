use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Something that can be used as a subject key.
pub(crate) trait Subject: Eq + Hash + Clone + Debug {}
impl<T: Eq + Hash + Clone + Debug> Subject for T {}

/// A synchronization and rate-limiting mechanism.
pub(crate) struct SubjectLimiter<S: Subject>(KindaPureSubjectLimiter<S>);

impl<S: Subject> SubjectLimiter<S> {
    /// Create a new [SubjectLimiter] which rate-limits functions per subject
    /// to be called no more than once per given `interval`.
    pub fn new(interval: Duration) -> Self {
        Self(KindaPureSubjectLimiter::new(
            Instant::now() - interval,
            interval,
        ))
    }

    /// Wraps the given async function `f`, calling it if it isn't currently
    /// running not has been called recently (within the duration specified
    /// to [`SubjectLimiter::new`]). Otherwise, does nothing (i.e. `f` is not called).
    pub fn lock(&self, subject: S) -> Option<Permit<S>> {
        self.0.lock(Instant::now(), subject)
    }

    /// Forget a subject. Blocks until the function running for the subject is done.
    /// Attempting to call [`SubjectLimiter::lock`] _while_ `forget` is running
    /// will do nothing (but calling [`SubjectLimiter::lock`] _after_ `forget` is
    /// done will re-insert the subject).
    pub async fn forget(&self, subject: &S) {
        self.0.forget(subject).await
    }
}

struct SubjectState {
    semaphore: Arc<Semaphore>,
    last_sent: Instant,
}

impl SubjectState {
    fn new(last_sent: Instant) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(1)),
            last_sent,
        }
    }
}

/// (Not actually) pure implementation of [SubjectLimiter].
///
/// In the past, I thought it would be easier to test [SubjectLimiter] if it were
/// implemented purely, but I changed my mind about that.
struct KindaPureSubjectLimiter<S: Subject> {
    subjects: Arc<Mutex<HashMap<S, SubjectState>>>,
    start: Instant,
    interval: Duration,
}

/// A [RAII](https://github.com/rust-unofficial/patterns/blob/main/src/patterns/behavioural/RAII.md)
/// for synchronization by calling [`SubjectLimiter::lock`].
pub(crate) struct Permit<S: Subject> {
    _permit: OwnedSemaphorePermit,
    subject: S,
    subjects: Arc<Mutex<HashMap<S, SubjectState>>>,
}

impl<S: Subject> Drop for Permit<S> {
    fn drop(&mut self) {
        let mut subjects = self.subjects.lock().unwrap();
        if let Some(state) = subjects.get_mut(&self.subject) {
            state.last_sent = Instant::now(); // impure
        }
    }
}

impl<S: Subject> KindaPureSubjectLimiter<S> {
    fn new(start: Instant, interval: Duration) -> Self {
        Self {
            subjects: Arc::new(Default::default()),
            start,
            interval,
        }
    }

    fn lock(&self, now: Instant, subject: S) -> Option<Permit<S>> {
        let mut subjects = self.subjects.lock().unwrap();
        let state = subjects
            .entry(subject.clone())
            .or_insert_with(|| SubjectState::new(self.start));
        if now - state.last_sent < self.interval {
            return None;
        }
        Arc::clone(&state.semaphore)
            .try_acquire_owned()
            .ok()
            .map(|permit| permit)
            .map(|permit| Permit {
                _permit: permit,
                subject,
                subjects: Arc::clone(&self.subjects),
            })
    }

    async fn forget(&self, subject: &S) {
        let acquire = {
            // note: don't want to keep self.subjects locked while awaiting on the semaphore.
            let subjects = self.subjects.lock().unwrap();
            subjects
                .get(subject)
                .map(|state| Arc::clone(&state.semaphore).acquire_owned())
        };
        // self.subjects RAII dropped, we can acquire the semaphore now.
        if let Some(acquire) = acquire {
            match acquire.await {
                Ok(_owned_permit) => {
                    let mut subjects = self.subjects.lock().unwrap();
                    if let Some(state) = subjects.remove(subject) {
                        state.semaphore.close();
                    }
                }
                Err(_) => {
                    tracing::warn!(
                        subject = format!("{subject:?}"),
                        "SubjectLimiter::forget called twice"
                    );
                }
            }
        } else {
            tracing::warn!(
                subject = format!("{subject:?}"),
                "SubjectLimiter::forget called on unknown subject"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::task::JoinHandle;

    #[tokio::test]
    async fn test_lock() {
        let interval = Duration::from_millis(100);
        let limiter = SubjectLimiter::new(interval);
        let task_a = create_task(&limiter, "subject1").expect(
            "task_a was not called, but it should have been called \
                because it was the first function.",
        );
        let task_b = create_task(&limiter, "subject1");
        assert!(
            task_b.is_none(),
            "task_b was called, but it should not have been called because task a recently ran."
        );
        let task_c = create_task(&limiter, "subject2").expect(
            "task_c was not called, but it should have been called \
                because it is a different subject than task_a.",
        );

        tokio::time::sleep(interval * 2).await;
        let task_d = create_task(&limiter, "subject1").expect(
            "task_d was not called, but it should have been called \
                because \"subject1\" has not been busy for a while.",
        );
        tokio::try_join!(task_a, task_c, task_d).unwrap();
    }

    fn create_task<S: Subject + Send + 'static>(
        limiter: &SubjectLimiter<S>,
        subject: S,
    ) -> Option<JoinHandle<()>> {
        if let Some(raii) = limiter.lock(subject) {
            let task = tokio::spawn(async move {
                let _raii_binding = raii;
                tokio::time::sleep(Duration::from_millis(10)).await;
            });
            Some(task)
        } else {
            None
        }
    }

    #[tokio::test]
    async fn test_forget_waits_until_unlocked() {
        let interval = Duration::from_millis(200);
        let limiter = SubjectLimiter::new(interval);
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let task_finished = Arc::new(Mutex::new(false));

        let task_a = {
            let raii = limiter.lock("subject1").unwrap();
            let task_finished = Arc::clone(&task_finished);
            tokio::spawn(async move {
                let _raii_binding = raii;
                started_tx.send(()).unwrap();
                tokio::time::sleep(Duration::from_millis(200)).await;
                *task_finished.lock().unwrap() = true;
            })
        };
        started_rx.await.unwrap();
        limiter.forget(&"subject1").await;
        assert!(Arc::into_inner(task_finished).unwrap().into_inner().unwrap());
        task_a.await.unwrap();
    }

    #[tokio::test]
    async fn test_forget_called_twice_shouldnt_blow_up() {
        let interval = Duration::from_millis(200);
        let limiter = SubjectLimiter::new(interval);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let task_a = {
            let raii = limiter.lock("subject1");
            tokio::spawn(async move {
                let _raii_binding = raii;
                tx.send(()).unwrap();
                tokio::time::sleep(Duration::from_millis(100)).await;
            })
        };
        rx.await.unwrap();
        tokio::join!(limiter.forget(&"subject1"), limiter.forget(&"subject1"));
        task_a.await.unwrap();
    }

    #[tokio::test]
    async fn test_different_subjects_not_locked() {
        let interval = Duration::from_millis(100);
        let limiter = SubjectLimiter::new(interval);
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let task_a = {
            let tx = tx.clone();
            let raii = limiter.lock("subject1").unwrap();
            tokio::spawn(async move {
                let _raii_binding = raii;
                tx.send(1).await.unwrap();
                tokio::time::sleep(Duration::from_millis(200)).await;
                tx.send(3).await.unwrap();
            })
        };
        let task_b = {
            let tx = tx.clone();
            let raii = limiter.lock("subject2").unwrap();
            tokio::spawn(async move {
                let _raii_binding = raii;
                tokio::time::sleep(Duration::from_millis(100)).await;
                tx.send(2).await.unwrap();
            })
        };
        tokio::try_join!(task_a, task_b).unwrap();
        let actual = [rx.recv().await, rx.recv().await, rx.recv().await];
        let expected = [Some(1), Some(2), Some(3)];
        assert_eq!(actual, expected);
    }
}
