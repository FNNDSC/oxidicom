use crate::assertions::*;
use crate::orthanc_client::orthanc_store;
use camino::Utf8Path;
use futures::StreamExt;
use oxidicom::{run_everything, DicomRsSettings, OxidicomEnvOptions};
use std::num::NonZeroUsize;
use std::time::Duration;

mod assertions;
mod orthanc_client;

const ORTHANC_URL: &str = "http://localhost:8042";
const CALLING_AE_TITLE: &str = "OXIDICOMTEST";

/// Runs the DICOM listener and pushes 2 series to it in parallel.
#[tokio::test(flavor = "multi_thread")]
async fn test_run_everything_from_env() {
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(tracing::Level::INFO)
            .finish(),
    )
    .unwrap();

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_dir_path = Utf8Path::from_path(temp_dir.path()).unwrap();

    let queue_name = names::Generator::default().next().unwrap();
    let options = create_test_options(temp_dir_path, queue_name.to_string());
    let amqp_address = options.amqp_address.clone();
    let (nats_shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel(1);
    let nats = async_nats::connect(options.nats_address.as_ref().unwrap())
        .await
        .unwrap();
    let mut subscriber = nats.subscribe("oxidicom.>").await.unwrap();
    let nats_subscriber_loop = tokio::spawn(async move {
        let mut messages = Vec::new();
        loop {
            tokio::select! {
                Some(v) = subscriber.next() => messages.push(v),
                Some(_) = shutdown_rx.recv() => break,
            }
        }
        messages
    });

    let (start_tx, start_rx) = tokio::sync::oneshot::channel();
    let on_start = move |x| start_tx.send(x).unwrap();

    let num_to_handle = Some(EXPECTED_SERIES.len());
    let server = run_everything(options, num_to_handle, Some(on_start));
    let server_handle = tokio::spawn(server);

    // wait for message from `on_start` indicating server is ready for connections
    start_rx.await.unwrap();

    // tell Orthanc to send the test data to us
    futures::stream::iter(EXPECTED_SERIES.iter().map(|s| s.SeriesInstanceUID.as_str()))
        .for_each_concurrent(2, |series_instance_uid| async move {
            let res = orthanc_store(ORTHANC_URL, CALLING_AE_TITLE, series_instance_uid)
                .await
                .unwrap();
            assert_eq!(res.failed_instances_count, 0);
        })
        .await;

    // wait for server to shut down
    server_handle.await.unwrap().unwrap();

    // Shutdown the NATS subscriber after waiting a little bit.
    // Note: instead of shutting itself down after receiving the correct number
    // of "DONE" messages, we prefer the naive approach of waiting 500ms instead,
    // so that here in the test we can assert that the "DONE" messages do indeed
    // come last and no out-of-order/race condition errors are happening.
    tokio::time::sleep(Duration::from_millis(500)).await;
    nats_shutdown_tx.send(true).await.unwrap();
    let lonk_messages = nats_subscriber_loop.await.unwrap();

    // run all assertions

    assert_lonk_messages(lonk_messages);

    tokio::join!(
        assert_files_stored(&temp_dir_path),
        assert_rabbitmq_messages(&amqp_address, &queue_name),
    );
}

fn create_test_options<P: AsRef<Utf8Path>>(
    files_root: P,
    queue_name: String,
) -> OxidicomEnvOptions {
    OxidicomEnvOptions {
        amqp_address: "amqp://localhost:5672".to_string(),
        files_root: files_root.as_ref().to_path_buf(),
        queue_name,
        nats_address: Some("localhost:4222".to_string()),
        progress_interval: Duration::from_millis(50),
        scp: DicomRsSettings {
            aet: "OXIDICOMTEST".to_string(),
            strict: false,
            uncompressed_only: false,
            promiscuous: true,
        },
        scp_max_pdu_length: 16384,
        listener_threads: NonZeroUsize::new(2).unwrap(),
        listener_port: 11112,
    }
}
