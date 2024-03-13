//! File mostly copied from dicom-rs.
//!
//! https://github.com/Enet4/dicom-rs/blob/dbd41ed3a0d1536747c6b8ea2b286e4c6e8ccc8a/storescp/src/main.rs

use std::net::{Ipv4Addr, SocketAddrV4};

use camino::Utf8PathBuf;
use tracing::Level;

use chris_scp::{ChrisPacsStorage, DicomRsConfig, run_server};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(Level::INFO)
            .finish(),
    )
    .unwrap_or_else(|e| {
        eprintln!(
            "Could not set up global logger: {}",
            snafu::Report::from_error(e)
        );
    });

    let address = SocketAddrV4::new(Ipv4Addr::from(0), 11111);
    let chris = ChrisPacsStorage::new(
        "http://chris:8000/api/v1/pacsfiles/".to_string(),
        "chris".to_string(),
        "chris1234".to_string(),
        Utf8PathBuf::from("/data"),
        3
    );
    let options = DicomRsConfig {
        calling_ae_title: "ChRIS".to_string(),
        strict: false,
        uncompressed_only: false,
        max_pdu_length: 16384,
    };
    run_server(&address, chris, options)
}
