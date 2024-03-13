//! File mostly copied from dicom-rs.
//!
//! https://github.com/Enet4/dicom-rs/blob/dbd41ed3a0d1536747c6b8ea2b286e4c6e8ccc8a/storescp/src/main.rs

use std::net::{Ipv4Addr, SocketAddrV4};

use camino::Utf8PathBuf;
use tracing::Level;

use oxidicom::{run_server, ChrisPacsStorage, DicomRsConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(if envmnt::is_or("CHRIS_VERBOSE", false) {
                Level::DEBUG
            } else {
                Level::INFO
            })
            .finish(),
    )
    .unwrap_or_else(|e| {
        eprintln!(
            "Could not set up global logger: {}",
            snafu::Report::from_error(e)
        );
    });

    let address = SocketAddrV4::new(Ipv4Addr::from(0), envmnt::get_u16("PORT", 11111));
    let chris = ChrisPacsStorage::new(
        format!("{}pacsfiles/", envmnt::get_or_panic("CHRIS_URL")),
        envmnt::get_or_panic("CHRIS_USERNAME"),
        envmnt::get_or_panic("CHRIS_PASSWORD"),
        Utf8PathBuf::from(envmnt::get_or_panic("CHRIS_FILES_ROOT")),
        envmnt::get_u16("CHRIS_HTTP_RETRIES", 3),
    );
    let options = DicomRsConfig {
        calling_ae_title: envmnt::get_or("CHRIS_SCP_AET", "ChRIS"),
        strict: envmnt::is_or("CHRIS_SCP_STRICT", false),
        uncompressed_only: envmnt::is_or("CHRIS_SCP_UNCOMPRESSED_ONLY", false),
        max_pdu_length: envmnt::get_u32("CHRIS_SCP_MAX_PDU_LENGTH", 16384),
    };
    run_server(&address, chris, options, false)
}
