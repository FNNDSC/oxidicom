use crate::enums::SeriesEvent;
use crate::error::HandleLoopError;
use crate::types::{SeriesCount, SeriesKey};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

/// Forward objects from `receiver` to the given `client`.
///
/// - Received `Some`: add item to the batch. When batch is full, give everything to the `client`
/// - Received `None`: flush current batch to the `client`
pub async fn cube_pacsfile_notifier(
    mut receiver: UnboundedReceiver<(SeriesKey, SeriesEvent<Result<(), ()>, SeriesCount>)>,
    celery: Arc<celery::Celery>,
) -> Result<(), HandleLoopError> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let receiver_loop = async {
        while let Some((series, event)) = receiver.recv().await {
            tx.send(handle_event(series, event, &celery)).unwrap();
        }
        drop(tx);
    };
    // join tasks and take note of any errors.
    let mut everything_ok = true;
    let joiner_loop = async {
        while let Some(task) = rx.recv().await {
            if task.await.unwrap().is_err() {
                everything_ok = false;
            }
        }
    };
    tokio::join!(receiver_loop, joiner_loop);
    if everything_ok {
        Ok(())
    } else {
        Err(HandleLoopError(
            "There was an error registering PACS files metadata to the database.",
        ))
    }
}

type RegistrationTask = JoinHandle<Result<(), ()>>;

fn handle_event(
    series: SeriesKey,
    event: SeriesEvent<Result<(), ()>, SeriesCount>,
    client: &Arc<celery::Celery>,
) -> RegistrationTask {
    match event {
        SeriesEvent::Instance(result) => tokio::spawn(async move {
            // dbg!((series, result));
            Ok(())
        }),
        SeriesEvent::Finish(count) => {
            let client = Arc::clone(client);
            tokio::spawn(
                async move { send_registration_task_to_celery(series, count, &client).await },
            )
        }
    }
}

async fn send_registration_task_to_celery(
    series: SeriesKey,
    count: SeriesCount,
    client: &celery::Celery,
) -> Result<(), ()> {
    match client.send_task(count.into_task()).await {
        Ok(r) => {
            tracing::info!(
                pacs_name = series.pacs_name.as_str(),
                SeriesInstanceUID = series.SeriesInstanceUID,
                celery_task_id = r.task_id,
                celery_task_name = "register_pacs_series"
            );
            Ok(())
        }
        Err(e) => {
            tracing::error!(
                pacs_name = series.pacs_name.as_str(),
                SeriesInstanceUID = series.SeriesInstanceUID,
                message = e.to_string()
            );
            Err(())
        }
    }
}
