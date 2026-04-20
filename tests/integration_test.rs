use crate::orthanc_client::orthanc_store;
use crate::util::assertions::*;
use crate::util::dicom_wo_studydate::{create_dicom_without_studydate, SERIES};
use crate::util::expected::EXPECTED_SERIES;
use crate::util::helpers::*;
use crate::util::send_dicom::store_one_dicom;
use camino::Utf8Path;
use futures::StreamExt;
use oxidicom::lonk::subject_of;
use oxidicom::run_everything;

mod orthanc_client;
mod util;

const ORTHANC_URL: &str = "http://localhost:8042";
const CALLING_AE_TITLE: &str = "OXIDICOMTEST";

/// Runs the DICOM listener and pushes 2 series to it in parallel.
#[tokio::test(flavor = "multi_thread")]
async fn test_run_everything_from_env() {
    init_logging();
    let path_str = "./tests/data".to_string();
    let temp_dir_path = Utf8Path::new(&path_str);

    let options = create_test_options(temp_dir_path, ROOT_SUBJECT.to_string(), 11112);
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

    tracing::info!(msg = "to orthanc store".to_string());

    // tell Orthanc to send the test data to us
    futures::stream::iter(EXPECTED_SERIES.iter().map(|s| s.SeriesInstanceUID.as_str()))
        .for_each_concurrent(2, |series_instance_uid| async move {
            tracing::info!(
                msg = "to orthanc store".to_string(),
                series_instance_uid = series_instance_uid,
            );
            let res = orthanc_store(ORTHANC_URL, CALLING_AE_TITLE, series_instance_uid)
                .await
                .unwrap();
            tracing::info!(
                msg = "done orthanc store".to_string(),
                series_instance_uid = series_instance_uid,
            );

            assert_eq!(res.failed_instances_count, 0);
        })
        .await;

    tracing::info!(msg = "to wait server handle".to_string());

    // wait for server to shut down
    server_handle.await.unwrap().unwrap();

    tracing::info!(msg = "after wait server handle".to_string());

    // get messages from NATS
    let lonk_messages = nats_subscriber_loop.await.unwrap();

    // run all assertions
    assert_files_stored(&temp_dir_path).await;
    assert_lonk_messages(lonk_messages);
    assert_cube_record("1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0".to_string())
        .await;
    assert_cube_record(
        "1.2.826.0.1.3680043.2.1143.515404396022363061013111326823367652".to_string(),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn test_missing_studydate_error_sent_to_nats() {
    // SMELL: lots of code duplication with test_run_everything_from_env
    init_logging();
    let path_str = "./tests/data".to_string();
    let temp_dir_path = Utf8Path::new(&path_str);
    let root_subject = "test-missing-studydate.oxidicom";
    let port = 11113;
    let options = create_test_options(temp_dir_path, root_subject.to_string(), port);
    let nats = async_nats::connect(options.nats_address.as_ref().unwrap())
        .await
        .unwrap();
    let subject = format!("{root_subject}.>");
    let mut subscriber = nats.subscribe(subject).await.unwrap();
    tracing::debug!("subscribed");
    let nats_subscriber_loop = tokio::spawn(async move {
        let mut messages = Vec::new();
        loop {
            tracing::debug!("waiting for next message...");
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

    let server = run_everything(options, Some(1), Some(on_start));
    let server_handle = tokio::spawn(server);

    // wait for message from `on_start` indicating server is ready for connections
    start_rx.await.unwrap();

    // send one DICOM data for storage
    let dcm = create_dicom_without_studydate();
    let address = format!("127.0.0.1:{port}");
    tracing::debug!("about to send DICOM");
    store_one_dicom(&address, dcm).await;

    // wait for server to shut down
    server_handle.await.unwrap().unwrap();

    // get messages from NATS
    let lonk_messages = nats_subscriber_loop.await.unwrap();
    let mut messages_iter = lonk_messages.into_iter();
    let message = messages_iter
        .next()
        .expect("Should have received one message from NATS");
    assert!(
        messages_iter.next().is_none(),
        "Should have only received 1 message from NATS, but received at least 2."
    );
    assert_eq!(
        message.subject.as_str(),
        subject_of(root_subject, &SERIES).as_str()
    );
}
