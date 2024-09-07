use crate::assertions::{assert_files_stored, assert_rabbitmq_messages, EXPECTED_SERIES};
use crate::orthanc_client::orthanc_store;
use camino::Utf8Path;
use futures::StreamExt;
use oxidicom::{run_everything, DicomRsSettings, OxidicomEnvOptions};
use std::num::NonZeroUsize;

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

    let queue_name = names::Generator::default().next().unwrap();
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_dir_path = Utf8Path::from_path(temp_dir.path()).unwrap();

    let num_to_handle = Some(EXPECTED_SERIES.len());
    let options = create_test_options(temp_dir_path, queue_name.to_string());
    let amqp_address = options.amqp_address.clone();
    let (start_tx, mut start_rx) = tokio::sync::mpsc::unbounded_channel();
    let on_start = move |x| start_tx.send(x).unwrap();
    let server = run_everything(options, num_to_handle, Some(on_start));
    let server_handle = tokio::spawn(server);

    // wait for message from `on_start` indicating server is ready for connections
    start_rx.recv().await.unwrap();

    futures::stream::iter(EXPECTED_SERIES.iter().map(|s| s.SeriesInstanceUID.as_str()))
        .for_each_concurrent(4, |series_instance_uid| async move {
            let res = orthanc_store(ORTHANC_URL, CALLING_AE_TITLE, series_instance_uid)
                .await
                .unwrap();
            assert_eq!(res.failed_instances_count, 0);
        })
        .await;
    server_handle.await.unwrap().unwrap();

    tokio::join!(
        assert_files_stored(&temp_dir_path),
        assert_rabbitmq_messages(&amqp_address, &queue_name)
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
        progress_nats_address: None,
        progress_interval: Default::default(),
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
