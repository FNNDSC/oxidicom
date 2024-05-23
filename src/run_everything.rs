use crate::dicomrs_options::ClientAETitle;
use crate::DicomRsConfig;
use std::collections::HashMap;
use std::net::SocketAddrV4;
use tokio::sync::mpsc;
use crate::listener_tcp_loop::dicom_listener_tcp_loop;
use crate::writer::dicom_storage_writer;

/// Runs everything:
///
/// 1. A TCP server loop to listen for incoming DICOM objects
/// 2. A file storage handler which writes DICOM files to disk
/// 3. A database connection pool which registers written files
pub async fn run_everything(
    address: SocketAddrV4,
    dicomrs_config: DicomRsConfig,
    pacs_addresses: HashMap<ClientAETitle, String>,
    max_pdu_length: usize,
    finite_connections: Option<usize>,
    listener_threads: usize,
) -> anyhow::Result<()> {
    let (tx_dcm, rx_dcm) = mpsc::unbounded_channel();
    let listener = tokio::task::spawn_blocking(move || dicom_listener_tcp_loop(
        address,
        dicomrs_config,
        finite_connections,
        listener_threads,
        max_pdu_length,
        tx_dcm,
        pacs_addresses
    ));
    let storage_writer = tokio::spawn(dicom_storage_writer(rx_dcm));
    let (r0, r1) = tokio::try_join!(listener, storage_writer)?;
    r0.and(r1)
}
