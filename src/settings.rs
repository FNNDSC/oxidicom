//! Oxidicom settings, which are configurable using environment variables.
use crate::dicomrs_settings::ClientAETitle;
use crate::DicomRsSettings;
use camino::Utf8PathBuf;
use serde::Deserialize;
use std::collections::HashMap;
use std::num::NonZeroUsize;

#[derive(Debug, Deserialize)]
pub struct OxidicomEnvOptions {
    pub amqp_address: String,
    pub files_root: Utf8PathBuf,
    pub progress_nats_address: String,
    #[serde(with = "humantime_serde")]
    pub progress_interval: std::time::Duration,
    pub scp: DicomRsSettings,
    #[serde(default)]
    pub scp_max_pdu_length: usize,
    #[serde(default)]
    pub pacs_address: HashMap<ClientAETitle, String>,
    #[serde(default = "default_listener_threads")]
    pub listener_threads: NonZeroUsize,
    #[serde(default = "default_listener_port")]
    pub listener_port: u16,
}



fn default_listener_threads() -> NonZeroUsize {
    NonZeroUsize::new(8).unwrap()
}

fn default_listener_port() -> u16 {
    11111
}
