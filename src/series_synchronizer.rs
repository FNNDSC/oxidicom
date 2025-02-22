use crate::enums::SeriesEvent;
use crate::error::HandleLoopError;
use futures::StreamExt;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

/// Waits on the [JoinHandle] of [PendingDicomInstance] for each `K`, so that
/// [SeriesEvent::Finish] is the last message to be sent to `sender` for the respective `K`.
pub(crate) async fn series_synchronizer<
    K: Eq + Hash + Send + Clone + std::fmt::Debug + 'static,
    T: Send + 'static,
    L: Send + 'static,
>(
    mut receiver: UnboundedReceiver<(K, SeriesEvent<JoinHandle<T>, L>)>,
    sender: UnboundedSender<(K, SeriesEvent<T, L>)>,
) -> Result<(), HandleLoopError> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let receiver_loop = async {
        let mut inflight_series: HashMap<K, Vec<_>> = Default::default();
        let sender = Arc::new(sender);
        while let Some((series, event)) = receiver.recv().await {
            match event {
                SeriesEvent::Instance(task) => {
                    enqueue_and_insert(series, task, &sender, &mut inflight_series)
                }
                SeriesEvent::Finish(final_message) => {
                    if let Some(tasks_for_series) = inflight_series.remove(&series) {
                        let sender = Arc::clone(&sender);
                        let task = tokio::task::spawn(async move {
                            wait_on_all_then_flush(tasks_for_series, &sender, series, final_message)
                                .await
                        });
                        tx.send(task).unwrap()
                    } else {
                        // FIXME THIS IS HAPPENING WHEN THE SAME SERIES IS BEING PUSHED MORE
                        // THAN ONCE AT THE SAME TIME. NEED TO DISCRIMINATE BETWEEN SERIES
                        // BY ASSOCIATION_ULID
                        tracing::error!(
                            series = format!("{series:?}"),
                            "No tasks were received for the series. This is a bug.",
                        );
                    }
                }
            }
        }
        drop(tx);
    };
    let mut everything_ok = true;
    let joiner_loop = async {
        while let Some(handle) = rx.recv().await {
            if let Err(e) = handle.await.unwrap() {
                tracing::error!("{}", e.to_string());
                everything_ok = false;
            }
        }
    };
    tokio::join!(receiver_loop, joiner_loop);
    if everything_ok {
        Ok(())
    } else {
        Err(HandleLoopError(
            "There was an error in registration_synchronizer",
        ))
    }
}

/// Create a task which joins the given `task`. If the given `task` is [Ok], send its return value
/// to `sender`.
///
/// Insert the created task into `inflight_series`.
fn enqueue_and_insert<
    K: Clone + Eq + Hash + Send + 'static,
    T: Send + 'static,
    F: Send + 'static,
>(
    series: K,
    task: JoinHandle<T>,
    sender: &Arc<UnboundedSender<(K, SeriesEvent<T, F>)>>,
    inflight_series: &mut HashMap<
        K,
        Vec<JoinHandle<Result<(), SendError<(K, SeriesEvent<T, F>)>>>>,
    >,
) {
    let sender = Arc::clone(sender);
    let series_clone = series.clone();
    let register_task = tokio::task::spawn(async move {
        sender.send((series_clone, SeriesEvent::Instance(task.await.unwrap())))
    });
    if let Some(v) = inflight_series.get_mut(&series) {
        v.push(register_task);
    } else {
        inflight_series.insert(series, vec![register_task]);
    }
}

/// Wait on all the tasks, then send [None] to `sender`.
async fn wait_on_all_then_flush<E: ToString, K, T, F>(
    tasks: Vec<JoinHandle<Result<(), E>>>,
    sender: &UnboundedSender<(K, SeriesEvent<T, F>)>,
    series: K,
    last: F,
) -> Result<(), SendError<(K, SeriesEvent<T, F>)>> {
    futures::stream::iter(tasks)
        .map(|handle| async { handle.await.unwrap() })
        .buffer_unordered(usize::MAX)
        .for_each(|result| async {
            if let Err(error) = result {
                tracing::error!("{}", error.to_string())
            }
        })
        .await;
    sender.send((series, SeriesEvent::Finish(last)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::mpsc::unbounded_channel;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_synchronizer() {
        let (source_tx, source_rx) = unbounded_channel();
        let (sink_tx, mut sink_rx) = unbounded_channel();
        let synchronizer = series_synchronizer(source_rx, sink_tx);
        let source = async move {
            source_tx.send(("A", dummy_task(100, "second"))).unwrap();
            source_tx.send(("A", dummy_task(150, "third"))).unwrap();
            source_tx.send(("A", dummy_task(50, "first"))).unwrap();
            source_tx
                .send(("A", SeriesEvent::Finish("finish")))
                .unwrap();
        };
        let sink = async move {
            assert_eq!(
                sink_rx.recv().await,
                Some(("A", SeriesEvent::Instance("first")))
            );
            assert_eq!(
                sink_rx.recv().await,
                Some(("A", SeriesEvent::Instance("second")))
            );
            assert_eq!(
                sink_rx.recv().await,
                Some(("A", SeriesEvent::Instance("third")))
            );
            assert_eq!(
                sink_rx.recv().await,
                Some(("A", SeriesEvent::Finish("finish")))
            );
        };
        let (_, _, result) = tokio::join!(source, sink, synchronizer);
        result.unwrap();
    }

    fn dummy_task<R: Send + 'static, F>(ms: u64, ret: R) -> SeriesEvent<JoinHandle<R>, F> {
        let duration = Duration::from_millis(ms);
        let task = tokio::spawn(async move {
            tokio::time::sleep(duration).await;
            ret
        });
        SeriesEvent::Instance(task)
    }
}
