use futures::StreamExt;

use oxidicom::run_everything_from_env;

use crate::assertions::run_assertions;
use crate::orthanc_client::orthanc_store;

mod assertions;
mod orthanc_client;

const EXAMPLE_SERIES_INSTANCE_UIDS: [&str; 2] = [
    // https://github.com/FNNDSC/SAG-anon
    "1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0",
    // https://github.com/datalad/example-dicom-structural
    "1.2.826.0.1.3680043.2.1143.515404396022363061013111326823367652",
];

const ORTHANC_URL: &str = "http://orthanc:8042";
const CALLING_AE_TITLE: &str = "OXIDICOMTEST";
const CALLED_AE_TITLE: &str = "OXITESTORTHANC";

/// Runs the DICOM listener and pushes 2 series to it in parallel.
#[tokio::test(flavor = "multi_thread")]
async fn test_run_everything_from_env() {
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(tracing::Level::INFO)
            .finish(),
    )
    .unwrap();

    let server_handle = tokio::spawn(run_everything_from_env(Some(
        EXAMPLE_SERIES_INSTANCE_UIDS.len(),
    )));
    // N.B. it might be necessary to wait for the TCP server to come up.
    // tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    let instances_counts: Vec<_> = futures::stream::iter(EXAMPLE_SERIES_INSTANCE_UIDS)
        .map(|series_instance_uid| async move {
            let res = orthanc_store(ORTHANC_URL, CALLING_AE_TITLE, series_instance_uid)
                .await
                .unwrap();
            assert_eq!(res.failed_instances_count, 0);
            res.instances_count
        })
        .buffered(4)
        .collect()
        .await;
    server_handle.await.unwrap().unwrap();
    run_assertions(&instances_counts).await;
}
