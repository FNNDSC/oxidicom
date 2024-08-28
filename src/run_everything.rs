use crate::association_series_state_loop::association_series_state_loop;
use crate::get_config;
use std::net::{Ipv4Addr, SocketAddrV4};
use tokio::sync::mpsc;

use crate::listener_tcp_loop::dicom_listener_tcp_loop;
use crate::notifier::cube_pacsfile_notifier;
use crate::registration_synchronizer::registration_synchronizer;
use crate::settings::OxidicomEnvOptions;
use futures::FutureExt;

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
        amqp_address, files_root, progress_nats_address, progress_interval, scp, scp_max_pdu_length, pacs_address, listener_threads, listener_port
    }: OxidicomEnvOptions,
    finite_connections: Option<usize>,
) -> anyhow::Result<()> {
    let celery = celery::app!(
        broker = AMQPBroker { amqp_address },
        tasks = [crate::registration_task::register_pacs_series],
        task_routes = [ "pacsfiles.tasks.register_pacs_series" => "main2" ],
    ).await?;

    let (tx_association, rx_association) = mpsc::unbounded_channel();
    let (tx_storetasks, rx_storetasks) = mpsc::unbounded_channel();
    let (tx_register, rx_register) = mpsc::unbounded_channel();
    let listener_handle = tokio::task::spawn_blocking(move || {
        dicom_listener_tcp_loop(
            SocketAddrV4::new(Ipv4Addr::from(0), listener_port),
            scp,
            finite_connections,
            listener_threads.get(),
            scp_max_pdu_length,
            tx_association,
            pacs_address,
        )
    });

    tokio::try_join!(
        association_series_state_loop(rx_association, tx_storetasks, files_root)
            .map(|r| r.unwrap()),
        registration_synchronizer(rx_storetasks, tx_register),
        cube_pacsfile_notifier(rx_register, celery)
    )?;
    listener_handle.await?
}
