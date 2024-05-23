use std::thread;

use crate::assertions::run_assertions;
use crate::orthanc_client::orthanc_store;
use oxidicom::run_everything_from_env;

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
#[test]
fn test_register_pacsfiles_to_cube() {
    let server_thread = thread::spawn(|| run_server_for_test(EXAMPLE_SERIES_INSTANCE_UIDS.len()));
    let instances_count: Vec<usize> = EXAMPLE_SERIES_INSTANCE_UIDS
        .iter()
        .map(|series_instance_uid| {
            thread::spawn(|| orthanc_store(ORTHANC_URL, CALLING_AE_TITLE, series_instance_uid))
        })
        .map(|thread| thread.join().unwrap().unwrap())
        .map(|res| {
            assert_eq!(res.failed_instances_count, 0);
            res.instances_count
        })
        .collect();

    server_thread.join().unwrap();
    run_assertions(&instances_count);
}

/// Create and run a server which will shut down after a given number of connections.
fn run_server_for_test(n_clients: usize) {
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(tracing::Level::INFO)
            .finish(),
    )
    .unwrap();
    let n_proc = thread::available_parallelism()
        .map(|n| std::cmp::min(n.get(), 8))
        .ok();
    run_everything_from_env(Some(n_clients), n_proc, Some(n_clients)).unwrap()
}
