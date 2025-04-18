use crate::association_series_state_loop::association_series_state_loop;
use std::net::{Ipv4Addr, SocketAddrV4};
use tokio::sync::mpsc;

use crate::celery_publisher::celery_publisher;
use crate::listener_tcp_loop::dicom_listener_tcp_loop;
use crate::lonk_publisher::lonk_publisher;
use crate::messenger::messenger;
use crate::series_synchronizer::series_synchronizer;
use crate::settings::OxidicomEnvOptions;
use futures::TryFutureExt;

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
    let (tx_lonk, rx_lonk) = mpsc::unbounded_channel();
    let (tx_celery, rx_celery) = mpsc::unbounded_channel();
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
    let celery_handle = tokio::spawn(async move {
        celery_publisher(rx_celery, &celery).await?;
        celery.close().await?;
        anyhow::Ok(())
    });
    let nats_handle = if let Some(client) = nats_client {
        tokio::spawn(async move {
            lonk_publisher(root_subject, &client, rx_lonk, progress_interval, dev_sleep).await?;
            client.flush().await?;
            client.drain().await?;
            anyhow::Ok(())
        })
    } else {
        tokio::spawn(async move {
            let mut rx = rx_lonk;
            while let Some(_) = rx.recv().await {}
            anyhow::Ok(())
        })
    };

    let result = tokio::try_join!(
        association_series_state_loop(rx_association, tx_storetasks, files_root, &tx_lonk)
            .map_err(anyhow::Error::from),
        series_synchronizer(rx_storetasks, tx_register).map_err(anyhow::Error::from),
        messenger(rx_register, &tx_lonk, &tx_celery).map_err(anyhow::Error::from)
    );
    listener_handle.await??;
    drop(tx_lonk);
    drop(tx_celery);
    celery_handle.await??;
    nats_handle.await??;
    result.map(|_| ())
}
