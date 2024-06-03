//! Oxidicom settings, which are configurable using environment variables.
use crate::dicomrs_settings::ClientAETitle;
use crate::DicomRsSettings;
use camino::Utf8PathBuf;
use serde::Deserialize;
use std::collections::HashMap;
use std::num::{NonZeroU32, NonZeroUsize};

#[derive(Debug, Deserialize)]
pub struct OxidicomEnvOptions {
    pub db: DatabaseOptions,
    pub files_root: Utf8PathBuf,
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

#[derive(Debug, Deserialize)]
pub struct DatabaseOptions {
    pub connection: String,
    #[serde(default = "default_pool_size")]
    pub pool: NonZeroU32,
    #[serde(default = "default_batch_size")]
    pub batch_size: NonZeroUsize,
}

fn default_pool_size() -> NonZeroU32 {
    NonZeroU32::new(10).unwrap()
}

fn default_batch_size() -> NonZeroUsize {
    NonZeroUsize::new(20).unwrap()
}

fn default_listener_threads() -> NonZeroUsize {
    NonZeroUsize::new(8).unwrap()
}

fn default_listener_port() -> u16 {
    11111
}
