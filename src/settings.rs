//! Oxidicom settings, which are configurable using environment variables.
use crate::DicomRsSettings;
use camino::Utf8PathBuf;
use serde::Deserialize;
use std::num::NonZeroUsize;

#[derive(Debug, Deserialize)]
pub struct OxidicomEnvOptions {
    pub amqp_address: String,
    pub files_root: Utf8PathBuf,
    #[serde(default = "default_queue_name")]
    pub queue_name: String,
    pub nats_address: Option<String>,
    #[serde(with = "humantime_serde", default = "default_progress_interval")]
    pub progress_interval: std::time::Duration,
    pub scp: DicomRsSettings,
    #[serde(default = "default_max_pdu_length")]
    pub scp_max_pdu_length: usize,
    #[serde(default = "default_listener_threads")]
    pub listener_threads: NonZeroUsize,
    #[serde(default = "default_listener_port")]
    pub listener_port: u16,
    #[serde(with = "humantime_serde")]
    pub dev_sleep: Option<std::time::Duration>,
}

/// The name of the queue used by the `register_pacs_series` celery task in *CUBE*'s code.
///
/// https://github.com/FNNDSC/ChRIS_ultron_backEnd/blob/b3cb0afa068b2cfb5a89eea22ff9b41437dc6f2a/chris_backend/core/celery.py#L36
fn default_queue_name() -> String {
    "main2".to_string()
}

fn default_listener_threads() -> NonZeroUsize {
    NonZeroUsize::new(8).unwrap()
}

fn default_listener_port() -> u16 {
    11111
}

fn default_progress_interval() -> std::time::Duration {
    std::time::Duration::from_nanos(1)
}

fn default_max_pdu_length() -> usize {
    16384
}
