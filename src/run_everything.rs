use crate::association_series_state_loop::association_series_state_loop;
use std::net::{Ipv4Addr, SocketAddrV4};
use tokio::sync::mpsc;

use crate::listener_tcp_loop::dicom_listener_tcp_loop;
use crate::notifier::cube_pacsfile_notifier;
use crate::series_synchronizer::series_synchronizer;
use crate::settings::OxidicomEnvOptions;
use futures::FutureExt;

/// Runs everything in parallel:
///
/// 1. A TCP server loop to listen for incoming DICOM objects
/// 2. A file storage handler which writes DICOM files to disk
/// 3. A notifier which sends events to CUBE as DICOM files are received
pub async fn run_everything<F>(
    OxidicomEnvOptions {
        amqp_address,
        files_root,
        nats_address,
        progress_interval,
        scp,
        scp_max_pdu_length,
        listener_threads,
        listener_port,
        queue_name,
        dev_sleep,
        root_subject,
    }: OxidicomEnvOptions,
    finite_connections: Option<usize>,
    on_start: Option<F>,
) -> anyhow::Result<()>
where
    F: FnOnce(SocketAddrV4) + Send + 'static,
{
    let celery = celery::app!(
        broker = AMQPBroker { amqp_address },
        tasks = [crate::registration_task::register_pacs_series],
        task_routes = [ "pacsfiles.tasks.register_pacs_series" => &queue_name ],
    )
    .await?;
    let nats_client = if let Some(address) = nats_address {
        Some(async_nats::connect(address).await?)
    } else {
        None
    };

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
            on_start,
        )
    });

    tokio::try_join!(
        association_series_state_loop(rx_association, tx_storetasks, files_root)
            .map(|r| r.unwrap()),
        series_synchronizer(rx_storetasks, tx_register),
        cube_pacsfile_notifier(
            rx_register,
            celery,
            nats_client,
            &root_subject,
            progress_interval,
            dev_sleep
        )
    )?;
    listener_handle.await?
}
