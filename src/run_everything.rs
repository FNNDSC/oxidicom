use std::collections::HashMap;
use std::net::SocketAddrV4;

use camino::Utf8PathBuf;
use tokio::sync::mpsc;

use crate::chrisdb_client::CubePostgresClient;
use crate::dicomrs_options::ClientAETitle;
use crate::listener_tcp_loop::dicom_listener_tcp_loop;
use crate::registerer::cube_pacsfile_registerer;
use crate::writer::dicom_storage_writer;
use crate::DicomRsConfig;

/// Runs everything in parallel:
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
    files_root: Utf8PathBuf,
    cubedb_client: CubePostgresClient,
    db_batch_size: usize,
) -> anyhow::Result<()> {
    let (tx_dcm, rx_dcm) = mpsc::unbounded_channel();
    let (tx_register, rx_register) = mpsc::unbounded_channel();
    let listener_handle = tokio::task::spawn_blocking(move || {
        dicom_listener_tcp_loop(
            address,
            dicomrs_config,
            finite_connections,
            listener_threads,
            max_pdu_length,
            tx_dcm,
            pacs_addresses,
        )
    });
    tokio::try_join!(
        dicom_storage_writer(rx_dcm, tx_register, files_root),
        cube_pacsfile_registerer(rx_register, cubedb_client, db_batch_size)
    )?;
    listener_handle.await?
}
