use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;

use crate::limiter::{LockError, SubjectLimiter};
use crate::lonk::{subject_of, Lonk};

/// Publishes LONK messages from the channel to NATS.
///
/// - All messages marked with [LonkPriority::Optional] over the interval
///   `progress_interval` are dropped, to avoid overflowing NATS.
/// - There is an optional delay specified by `sleep` which throttles
///   performance. The only purpose of this is for debugging clients
///   of NATS progress notifications such as ChRIS_ui.
pub(crate) async fn lonk_publisher(
    root_subject: String,
    client: &async_nats::Client,
    mut rx: UnboundedReceiver<PublishLonkParams>,
    progress_interval: Duration,
    sleep: Option<Duration>,
) -> Result<(), async_nats::PublishError> {
    let limiter = SubjectLimiter::new(progress_interval);
    while let Some(PublishLonkParams { lonk, priority }) = rx.recv().await {
        let subject = subject_of(&root_subject, &lonk.series);
        if matches!(priority, LonkPriority::Last) {
            limiter.forget(&subject).await;
        }
        if matches!(priority, LonkPriority::Required | LonkPriority::Last) {
            send_lonk(subject, lonk, client).await?;
        } else {
            limited_send_lonk(subject, lonk, client, &limiter).await?;
        }
        if let Some(sleep_duration) = sleep {
            tracing::info!("OXIDICOM_DEV_SLEEP is set, sleeping for {:?}. Please unset this option in production!", sleep_duration);
            tokio::time::sleep(sleep_duration).await;
        }
    }
    Ok(())
}

async fn send_lonk(
    subject: String,
    Lonk { series, message }: Lonk,
    client: &async_nats::Client,
) -> Result<(), async_nats::PublishError> {
    let payload = message.into_bytes();
    tracing::debug!(
        SeriesInstanceUID = &series.SeriesInstanceUID,
        pacs_name = series.pacs_name.as_str(),
        payload = payload
            .iter()
            .map(|b| format!("{b:#04x}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    client.publish(subject, payload).await
}

async fn limited_send_lonk(
    subject: String,
    lonk: Lonk,
    client: &async_nats::Client,
    limiter: &SubjectLimiter<String>,
) -> Result<(), async_nats::PublishError> {
    match limiter.lock(subject.clone()) {
        Ok(_raii) => send_lonk(subject, lonk, client).await,
        Err(error) => {
            let series = &lonk.series;
            let reason = match error {
                LockError::TooSoon => "a prior notification was sent recently",
                LockError::Busy => "a prior notification is currently being sent",
            };
            tracing::trace!(
                SeriesInstanceUID = series.SeriesInstanceUID,
                pacs_name = series.pacs_name.as_str(),
                reason = reason,
                "Notification skipped.",
            );
            Ok(())
        }
    }
}

/// Parameters for how to publish a LONK message.
pub(crate) struct PublishLonkParams {
    pub lonk: Lonk,
    pub(crate) priority: LonkPriority,
}

impl PublishLonkParams {
    pub fn optional(lonk: Lonk) -> Self {
        Self {
            lonk,
            priority: LonkPriority::Optional,
        }
    }
    pub fn required(lonk: Lonk) -> Self {
        Self {
            lonk,
            priority: LonkPriority::Required,
        }
    }
    pub fn last(lonk: Lonk) -> Self {
        Self {
            lonk,
            priority: LonkPriority::Last,
        }
    }
}

/// Priority level of a LONK message.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum LonkPriority {
    /// A LONK message which _may_ be dropped.
    Optional,
    /// A LONK message which _must_ be published.
    Required,
    /// A LONK message which _must_ be published, and is the last message of its DICOM series.
    Last,
}
