use crate::server::run_server;
use crate::{ChrisPacsStorage, DicomRsConfig};
use camino::Utf8PathBuf;
use std::error::Error;
use std::net::{Ipv4Addr, SocketAddrV4};

/// Calls [run_server] using configuration from environment variables.
pub fn run_server_from_env(
    n_threads: Option<usize>,
    finite_connections: Option<usize>,
) -> Result<(), Box<dyn Error>> {
    let address = SocketAddrV4::new(Ipv4Addr::from(0), envmnt::get_u16("PORT", 11111));
    let pacs_name = env_option("CHRIS_PACS_NAME");
    let chris = ChrisPacsStorage::new(
        format!("{}pacsfiles/", envmnt::get_or_panic("CHRIS_URL")),
        envmnt::get_or_panic("CHRIS_USERNAME"),
        envmnt::get_or_panic("CHRIS_PASSWORD"),
        Utf8PathBuf::from(envmnt::get_or_panic("CHRIS_FILES_ROOT")),
        envmnt::get_u16("CHRIS_HTTP_RETRIES", 3),
        pacs_name,
    );
    let options = DicomRsConfig {
        calling_ae_title: envmnt::get_or("CHRIS_SCP_AET", "ChRIS"),
        strict: envmnt::is_or("CHRIS_SCP_STRICT", false),
        uncompressed_only: envmnt::is_or("CHRIS_SCP_UNCOMPRESSED_ONLY", false),
        max_pdu_length: envmnt::get_u32("CHRIS_SCP_MAX_PDU_LENGTH", 16384),
    };

    let n_threads = n_threads.unwrap_or_else(|| envmnt::get_usize("CHRIS_SCP_THREADS", 16));
    run_server(&address, chris, options, finite_connections, n_threads)
}

fn env_option(name: &'static str) -> Option<String> {
    let value = envmnt::get_or(name, "");
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
