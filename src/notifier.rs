use crate::enums::SeriesEvent;
use crate::error::{DicomStorageError, HandleLoopError};
use crate::limiter::SubjectLimiter;
use crate::lonk::{done_message, error_message, progress_message, subject_of};
use crate::types::{DicomInfo, SeriesKey, SeriesPath};
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

type SeriesCounts = HashMap<SeriesKey, u32>;

/// Forward objects from `receiver` to the given `client`.
///
/// - Received `Some`: add item to the batch. When batch is full, give everything to the `client`
/// - Received `None`: flush current batch to the `client`
pub async fn cube_pacsfile_notifier(
    mut receiver: UnboundedReceiver<(
        SeriesKey,
        SeriesEvent<Result<(), DicomStorageError>, DicomInfo<SeriesPath>>,
    )>,
    celery: Arc<celery::Celery>,
    nats_client: Option<async_nats::Client>,
    progress_interval: Duration,
) -> Result<(), HandleLoopError> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let receiver_loop = async {
        let mut counts: SeriesCounts = Default::default();
        let limiter = Arc::new(SubjectLimiter::new(progress_interval));
        while let Some((series, event)) = receiver.recv().await {
            let task = handle_event(
                &mut counts,
                series,
                event,
                &celery,
                nats_client.clone(),
                &limiter,
            );
            if let Some(task) = task {
                tx.send(task).unwrap();
            }
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
    counts: &mut SeriesCounts,
    series_key: SeriesKey,
    event: SeriesEvent<Result<(), DicomStorageError>, DicomInfo<SeriesPath>>,
    celery_client: &Arc<celery::Celery>,
    nats_client: Option<async_nats::Client>,
    limiter: &Arc<SubjectLimiter<SeriesKey>>,
) -> Option<RegistrationTask> {
    match event {
        SeriesEvent::Instance(result) => {
            let payload = count_series(series_key.clone(), counts, result);
            limiter.lock(series_key.clone()).map(|raii| {
                tokio::spawn(async move {
                    let _raii_binding = raii;
                    maybe_send_lonk(nats_client, &series_key, payload).await
                })
            })
        }
        SeriesEvent::Finish(series_info) => {
            let celery_client = Arc::clone(celery_client);
            let limiter = Arc::clone(limiter);
            let ndicom = counts.remove(&series_key).unwrap_or(0);
            let task = tokio::spawn(async move {
                limiter.forget(&series_key).await;
                let (a, b) = tokio::join!(
                    maybe_send_final_progress_messages(nats_client, &series_key, ndicom),
                    send_registration_task_to_celery(series_info, ndicom, &celery_client)
                );
                a.and(b)
            });
            Some(task)
        }
    }
}

/// If `result` is success: increment the count for the series.
/// Returns a message which _oxidicom_ should send to NATS conveying the status of `result`.
fn count_series(
    series: SeriesKey,
    counts: &mut SeriesCounts,
    result: Result<(), DicomStorageError>,
) -> Bytes {
    match result {
        Ok(_) => {
            let count = counts.entry(series).or_insert(0);
            *count += 1;
            progress_message(*count)
        }
        Err(e) => error_message(e),
    }
}

async fn maybe_send_lonk(
    client: Option<async_nats::Client>,
    series: &SeriesKey,
    payload: Bytes,
) -> Result<(), ()> {
    if let Some(client) = client {
        send_lonk(client, series, payload).await.map_err(|e| {
            tracing::error!(error = e.to_string());
            ()
        })
    } else {
        Ok(())
    }
}

async fn send_lonk(
    client: async_nats::Client,
    series: &SeriesKey,
    payload: Bytes,
) -> Result<(), async_nats::PublishError> {
    client.publish(subject_of(series), payload).await
}

async fn maybe_send_final_progress_messages(
    client: Option<async_nats::Client>,
    series: &SeriesKey,
    ndicom: u32,
) -> Result<(), ()> {
    if let Some(client) = client {
        send_final_progress_messages(client, series, ndicom)
            .await
            .map_err(|e| {
                tracing::error!(error = e.to_string());
                ()
            })
    } else {
        Ok(())
    }
}

async fn send_final_progress_messages(
    client: async_nats::Client,
    series: &SeriesKey,
    ndicom: u32,
) -> Result<(), async_nats::PublishError> {
    let subject = subject_of(series);
    client
        .publish(subject.clone(), progress_message(ndicom))
        .await?;
    client.publish(subject, done_message()).await
}

async fn send_registration_task_to_celery(
    series: DicomInfo<SeriesPath>,
    ndicom: u32,
    client: &celery::Celery,
) -> Result<(), ()> {
    let pacs_name = series.pacs_name.clone();
    let series_instance_uid = series.SeriesInstanceUID.clone();
    let task = series.into_task(ndicom);
    match client.send_task(task).await {
        Ok(r) => {
            tracing::info!(
                pacs_name = pacs_name.as_str(),
                SeriesInstanceUID = series_instance_uid,
                celery_task_id = r.task_id,
                celery_task_name = "register_pacs_series"
            );
            Ok(())
        }
        Err(e) => {
            tracing::error!(
                pacs_name = pacs_name.as_str(),
                SeriesInstanceUID = series_instance_uid,
                message = e.to_string()
            );
            Err(())
        }
    }
}
