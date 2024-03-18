use crate::cube_client::CubePacsStorageClient;
use crate::dicomrs_options::OurAETitle;
use crate::server::run_server;
use crate::DicomRsConfig;
use anyhow::Context;
use camino::Utf8PathBuf;
use std::net::{Ipv4Addr, SocketAddrV4};

/// Calls [run_server] using configuration from environment variables.
///
/// Function parameters are prioritized over environment variable values.
pub fn run_server_from_env(
    listener_threads: Option<usize>,
    pusher_threads: Option<usize>,
    finite_connections: Option<usize>,
) -> anyhow::Result<()> {
    let address = SocketAddrV4::new(Ipv4Addr::from(0), envmnt::get_u16("PORT", 11111));
    let chris = CubePacsStorageClient::new(
        format!("{}pacsfiles/", envmnt::get_or_panic("CHRIS_URL")),
        envmnt::get_or_panic("CHRIS_USERNAME"),
        envmnt::get_or_panic("CHRIS_PASSWORD"),
        Utf8PathBuf::from(envmnt::get_or_panic("CHRIS_FILES_ROOT")),
        envmnt::get_u16("CHRIS_HTTP_RETRIES", 3),
    );
    let dicomrs_config = DicomRsConfig {
        aet: OurAETitle::from(envmnt::get_or("CHRIS_SCP_AET", "ChRIS")),
        strict: envmnt::is_or("CHRIS_SCP_STRICT", false),
        uncompressed_only: envmnt::is_or("CHRIS_SCP_UNCOMPRESSED_ONLY", false),
    };

    let pacs_address = env_option("CHRIS_PACS_ADDRESS")
        .map(|a| a.parse())
        .transpose()
        .context("Invalid value for CHRIS_PACS_ADDRESS")?;
    let listener_threads =
        listener_threads.unwrap_or_else(|| envmnt::get_usize("CHRIS_LISTENER_THREADS", 16));
    let pusher_threads =
        pusher_threads.unwrap_or_else(|| envmnt::get_usize("CHRIS_PUSHER_THREADS", 4));
    let max_pdu_length = envmnt::get_usize("CHRIS_SCP_MAX_PDU_LENGTH", 16384);
    run_server(
        address,
        chris,
        dicomrs_config,
        pacs_address,
        max_pdu_length,
        finite_connections,
        listener_threads,
        pusher_threads,
    )
}

fn env_option(name: &'static str) -> Option<String> {
    let value = envmnt::get_or(name, "");
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
