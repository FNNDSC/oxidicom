use crate::assertions::*;
use crate::orthanc_client::orthanc_store;
use camino::Utf8Path;
use futures::StreamExt;
use oxidicom::{run_everything, DicomRsSettings, OxidicomEnvOptions};
use std::num::NonZeroUsize;
use std::sync::Once;
use std::time::Duration;

mod assertions;
mod orthanc_client;

const ORTHANC_URL: &str = "http://localhost:8042";
const CALLING_AE_TITLE: &str = "OXIDICOMTEST";

static INIT_LOGGING: Once = Once::new();

/// Runs the DICOM listener and pushes 2 series to it in parallel.
#[tokio::test(flavor = "multi_thread")]
async fn test_run_everything_from_env() {
    init_logging();
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_dir_path = Utf8Path::from_path(temp_dir.path()).unwrap();

    let queue_name = names::Generator::default().next().unwrap();
    let options = create_test_options(temp_dir_path, queue_name.to_string());
    let amqp_address = options.amqp_address.clone();
    let nats = async_nats::connect(options.nats_address.as_ref().unwrap())
        .await
        .unwrap();
    let subject = format!("{ROOT_SUBJECT}.>");
    let mut subscriber = nats.subscribe(subject).await.unwrap();
    let nats_subscriber_loop = tokio::spawn(async move {
        let mut messages = Vec::new();
        loop {
            // Loop until no more messages received for a while.
            tokio::select! {
                Some(v) = subscriber.next() => messages.push(v),
                _ = tokio::time::sleep(sleep_duration()) => break,
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

    // get messages from NATS
    let lonk_messages = nats_subscriber_loop.await.unwrap();

    // run all assertions
    assert_files_stored(&temp_dir_path).await;
    assert_rabbitmq_messages(&amqp_address, &queue_name).await;
    assert_lonk_messages(lonk_messages);
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
        dev_sleep: None,
        root_subject: ROOT_SUBJECT.to_string(),
    }
}

fn sleep_duration() -> Duration {
    if matches!(option_env!("CI"), Some("true")) {
        Duration::from_secs(10)
    } else {
        Duration::from_millis(500)
    }
}

fn init_logging() {
    INIT_LOGGING.call_once(|| {
        tracing::subscriber::set_global_default(
            tracing_subscriber::FmtSubscriber::builder()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .finish(),
        )
        .unwrap()
    })
}
