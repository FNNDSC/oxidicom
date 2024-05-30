use crate::chrisdb_client::CubePostgresClient;
use crate::get_config;
use sqlx::postgres::PgPoolOptions;
use std::net::{Ipv4Addr, SocketAddrV4};
use tokio::sync::mpsc;

use crate::listener_tcp_loop::dicom_listener_tcp_loop;
use crate::registerer::cube_pacsfile_registerer;
use crate::settings::OxidicomEnvOptions;
use crate::writer::dicom_storage_writer;

/// Calls [run_everything] using configuration from environment variables.
///
/// Function parameters are prioritized over environment variable values.
///
/// `finite_connections`: shut down the server after the given number of DICOM associations.
pub async fn run_everything_from_env(finite_connections: Option<usize>) -> anyhow::Result<()> {
    let config = get_config();
    let settings = config.extract()?;
    run_everything(settings, finite_connections).await
}

/// Runs everything in parallel:
///
/// 1. A TCP server loop to listen for incoming DICOM objects
/// 2. A file storage handler which writes DICOM files to disk
/// 3. A database connection pool which registers written files
async fn run_everything(
    OxidicomEnvOptions {
        db,
        files_root,
        scp,
        scp_max_pdu_length,
        pacs_address,
        listener_threads,
        listener_port,
    }: OxidicomEnvOptions,
    finite_connections: Option<usize>,
) -> anyhow::Result<()> {
    let db_pool = PgPoolOptions::new()
        .max_connections(db.pool.get())
        .connect(&db.connection)
        .await?;
    let cubedb_client = CubePostgresClient::new(db_pool, None);

    let (tx_dcm, rx_dcm) = mpsc::unbounded_channel();
    let (tx_register, rx_register) = mpsc::unbounded_channel();
    let listener_handle = tokio::task::spawn_blocking(move || {
        dicom_listener_tcp_loop(
            SocketAddrV4::new(Ipv4Addr::from(0), listener_port),
            scp,
            finite_connections,
            listener_threads.get(),
            scp_max_pdu_length,
            tx_dcm,
            pacs_address,
        )
    });
    tokio::try_join!(
        dicom_storage_writer(rx_dcm, tx_register, files_root),
        cube_pacsfile_registerer(rx_register, cubedb_client, db.batch_size.get())
    )?;
    listener_handle.await?
}
