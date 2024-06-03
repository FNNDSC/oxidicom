use crate::enums::PendingRegistration;
use crate::error::HandleLoopError;
use crate::pacs_file::PacsFileRegistrationRequest;
use crate::series_key_set::SeriesKeySet;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

/// `registration_synchronizer` is intended as a way to synchronize requests before they are
/// sent to [crate::registerer::cube_pacsfile_registerer]. It guarantees that the "flush" command
/// can be invoked after all tasks for an association are complete.
pub(crate) async fn registration_synchronizer(
    mut receiver: UnboundedReceiver<(SeriesKeySet, PendingRegistration)>,
    sender: UnboundedSender<Option<PacsFileRegistrationRequest>>,
) -> Result<(), HandleLoopError> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let receiver_loop = async {
        let mut inflight_series: HashMap<SeriesKeySet, Vec<_>> = Default::default();
        let sender = Arc::new(sender);
        while let Some((series, event)) = receiver.recv().await {
            match event {
                PendingRegistration::Task(task) => {
                    enqueue_registration_and_insert(series, task, &sender, &mut inflight_series)
                }
                PendingRegistration::End => {
                    let tasks_for_series = inflight_series
                        .remove(&series)
                        .expect("No tasks were received for the series.");
                    let sender = Arc::clone(&sender);
                    let task = tokio::task::spawn(async move {
                        wait_on_all_then_flush(tasks_for_series, &sender).await
                    });
                    tx.send(task).unwrap()
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

/// Create a task which joins the given `task`. If the given `task` is [Ok], send the
/// [PacsFileRegistrationRequest] to `sender`.
///
/// Insert the created task into `inflight_series`.
fn enqueue_registration_and_insert(
    series: SeriesKeySet,
    task: JoinHandle<Result<PacsFileRegistrationRequest, ()>>,
    sender: &Arc<UnboundedSender<Option<PacsFileRegistrationRequest>>>,
    inflight_series: &mut HashMap<
        SeriesKeySet,
        Vec<JoinHandle<Result<(), SendError<Option<PacsFileRegistrationRequest>>>>>,
    >,
) {
    let sender = Arc::clone(&sender);
    let register_task = tokio::task::spawn(async move {
        if let Ok(pacs_file) = task.await.unwrap() {
            sender.send(Some(pacs_file))
        } else {
            Ok(())
        }
    });
    if let Some(v) = inflight_series.get_mut(&series) {
        v.push(register_task);
    } else {
        inflight_series.insert(series, vec![register_task]);
    }
}

/// Wait on all the tasks, then send [None] to `sender`.
async fn wait_on_all_then_flush<E: ToString, P>(
    tasks: Vec<JoinHandle<Result<(), E>>>,
    sender: &UnboundedSender<Option<P>>,
) -> Result<(), SendError<Option<P>>> {
    futures::stream::iter(tasks)
        .map(|handle| async { handle.await.unwrap() })
        .buffer_unordered(usize::MAX)
        .for_each(|result| async {
            if let Err(error) = result {
                tracing::error!("{}", error.to_string())
            }
        })
        .await;
    sender.send(None)
}
