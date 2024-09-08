use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// A synchronization and rate-limiting mechanism.
pub(crate) struct SubjectLimiter<S: Eq + Hash + Clone + tracing::Value>(PureSubjectLimiter<S>);

impl<S: Eq + Hash + Clone + tracing::Value> SubjectLimiter<S> {
    /// Create a new [SubjectLimiter] which rate-limits functions per subject
    /// to be called no more than once per given `interval`.
    pub fn new(interval: Duration) -> Self {
        Self(PureSubjectLimiter::new(Instant::now() - interval, interval))
    }

    /// Wraps the given async function `f`, calling it if it isn't currently
    /// running not has been called recently (within the duration specified
    /// to [`SubjectLimiter::new`]). Otherwise, does nothing (i.e. `f` is not called).
    pub async fn lock<Fut, R>(&self, subject: S, f: Fut) -> Option<R>
    where
        Fut: Future<Output = R>,
    {
        self.0
            .lock(Instant::now(), || Instant::now(), subject, f)
            .await
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

/// Pure implementation of [SubjectLimiter].
struct PureSubjectLimiter<S: Eq + Hash + Clone + tracing::Value> {
    subjects: std::sync::Mutex<HashMap<S, SubjectState>>,
    start: Instant,
    interval: Duration,
}

impl<S: Eq + Hash + Clone + tracing::Value> PureSubjectLimiter<S> {
    fn new(start: Instant, interval: Duration) -> Self {
        Self {
            subjects: Default::default(),
            start,
            interval,
        }
    }

    async fn lock<L, Fut, R>(&self, now: Instant, later: L, subject: S, f: Fut) -> Option<R>
    where
        L: FnOnce() -> Instant,
        Fut: Future<Output = R>,
    {
        let (try_acquire, last_sent) = {
            // note: don't want to keep self.subjects locked while running `f`
            let mut subjects = self.subjects.lock().unwrap();
            let state = subjects
                .entry(subject.clone())
                .or_insert_with(|| SubjectState::new(self.start));
            let permit = Arc::clone(&state.semaphore).try_acquire_owned();
            (permit, state.last_sent)
        };
        if now - last_sent < self.interval {
            return None;
        }
        if let Ok(_permit_raii) = try_acquire {
            let ret = f.await;
            let mut subjects = self.subjects.lock().unwrap();
            if let Some(state) = subjects.get_mut(&subject) {
                state.last_sent = later();
            }
            Some(ret)
        } else {
            None
        }
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
                    tracing::warn!(subject = subject, "SubjectLimiter::forget called twice");
                }
            }
        } else {
            tracing::warn!(
                subject = subject,
                "SubjectLimiter::forget called on unknown subject"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::task::JoinHandle;

    #[tokio::test]
    async fn test_lock() {
        let interval = Duration::from_millis(100);
        let limiter = Arc::new(SubjectLimiter::new(interval));
        let task_a = create_task(&limiter, "subject1");
        let task_b = create_task(&limiter, "subject1");
        let task_c = create_task(&limiter, "subject2");

        let (ret_a, ret_b, ret_c) = tokio::try_join!(task_a, task_b, task_c).unwrap();
        assert!(
            ret_a.is_some(),
            "task_a was not called, but it should have been called because it was the first function."
        );
        assert!(
            ret_b.is_none(),
            "task_b was called, but it should not have been called because task a recently ran."
        );
        assert!(
            ret_c.is_some(),
            "task_c was not called, but it should have been called because it is a different subject than task_a."
        );
        tokio::time::sleep(interval).await;
        let task_d = create_task(&limiter, "subject1");
        let ret_d = task_d.await.unwrap();
        assert!(
            ret_d.is_some(),
            "task_d was not called, but it should have been called because \"subject1\" has not been busy for a while."
        );
    }

    fn create_task<S: Eq + Hash + Send + Clone + tracing::Value + 'static>(
        limiter: &Arc<SubjectLimiter<S>>,
        subject: S,
    ) -> JoinHandle<Option<()>> {
        let limiter = Arc::clone(&limiter);
        tokio::spawn(async move {
            limiter
                .lock(subject, async {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                })
                .await
        })
    }

    #[tokio::test]
    async fn test_lock_during_busy_forget() {
        let interval = Duration::from_millis(200);
        let limiter = Arc::new(SubjectLimiter::new(interval));
        let finished = Arc::new(std::sync::Mutex::new(false));
        let (tx, rx) = tokio::sync::oneshot::channel();
        let start = Instant::now();
        let task_a = {
            let limiter = Arc::clone(&limiter);
            let finished = Arc::clone(&finished);
            let start = start.clone();
            tokio::spawn(async move {
                limiter
                    .lock("subject1", async {
                        tx.send(()).unwrap();
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        *finished.lock().unwrap() = true;
                        start.elapsed()
                    })
                    .await
            })
        };
        rx.await.unwrap();
        limiter.forget(&"subject1").await;
        let outer_elapsed = start.elapsed();
        let task_a_elapsed = task_a.await.unwrap().unwrap();
        assert!(
            outer_elapsed >= task_a_elapsed,
            "SubjectLimiter::forget should have taken as long as task_a slept for, \
            because it should wait on task_a to finish. \
            outer_elapsed={outer_elapsed:?} task_a_elapsed={task_a_elapsed:?}"
        );
        assert!(*finished.lock().unwrap());
    }

    #[tokio::test]
    async fn test_forget_called_twice_shouldnt_blow_up() {
        let interval = Duration::from_millis(200);
        let limiter = Arc::new(SubjectLimiter::new(interval));
        let (tx, rx) = tokio::sync::oneshot::channel();
        let task_a = {
            let limiter = Arc::clone(&limiter);
            tokio::spawn(async move {
                limiter
                    .lock("subject1", async {
                        tx.send(()).unwrap();
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    })
                    .await
            })
        };
        rx.await.unwrap();
        tokio::join!(limiter.forget(&"subject1"), limiter.forget(&"subject1"));
        task_a.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn test_different_subjects_not_locked() {
        let interval = Duration::from_millis(100);
        let limiter = Arc::new(SubjectLimiter::new(interval));
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let task_a = {
            let limiter = Arc::clone(&limiter);
            let tx = tx.clone();
            tokio::spawn(async move {
                limiter
                    .lock("subject1", async {
                        tx.send(1).await.unwrap();
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        tx.send(3).await.unwrap();
                    })
                    .await
            })
        };
        let task_b = {
            let limiter = Arc::clone(&limiter);
            let tx = tx.clone();
            tokio::spawn(async move {
                limiter
                    .lock("subject2", async {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        tx.send(2).await.unwrap();
                    })
                    .await
            })
        };
        let (a, b) = tokio::try_join!(task_a, task_b).unwrap();
        assert!(a.is_some());
        assert!(b.is_some());
        let actual = [rx.recv().await, rx.recv().await, rx.recv().await];
        let expected = [Some(1), Some(2), Some(3)];
        assert_eq!(actual, expected);
    }
}
