use crate::types::{DicomInfo, SeriesPath};
use celery::error::CeleryError;
use celery::Celery;
use tokio::sync::mpsc::UnboundedReceiver;

pub(crate) type CubeRegistrationParams = (DicomInfo<SeriesPath>, u32);

/// Creates a celery task of `register_pacs_series` for the data received from the channel.
pub(crate) async fn celery_publisher(
    mut rx: UnboundedReceiver<CubeRegistrationParams>,
    client: &Celery,
) -> Result<(), CeleryError> {
    while let Some((series, ndicom)) = rx.recv().await {
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
            }
            Err(e) => {
                tracing::error!(
                    pacs_name = pacs_name.as_str(),
                    SeriesInstanceUID = series_instance_uid,
                    message = e.to_string()
                );
                return Err(e);
            }
        }
    }
    Ok(())
}
