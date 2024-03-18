use std::thread;

use crate::orthanc_client::orthanc_store;
use oxidicom::run_server_from_env;

mod orthanc_client;

const EXAMPLE_SERIES_INSTANCE_UIDS: [&str; 2] = [
    // https://github.com/FNNDSC/SAG-anon
    "1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0",
    // https://github.com/datalad/example-dicom-structural
    "1.2.826.0.1.3680043.2.1143.515404396022363061013111326823367652",
];

const ORTHANC_URL: &str = "http://orthanc:8042";
const AE_TITLE: &str = "OXIDICOMTEST";

/// Runs the DICOM listener and pushes 2 series to it in parallel.
#[test]
fn test_register_pacsfiles_to_cube() {
    let cube_url = envmnt::get_or_panic("CHRIS_URL");
    let username = envmnt::get_or_panic("CHRIS_USERNAME");
    let password = envmnt::get_or_panic("CHRIS_PASSWORD");

    let server_thread = thread::spawn(|| run_server_for_test(EXAMPLE_SERIES_INSTANCE_UIDS.len()));
    let total_dicom_instances: usize = EXAMPLE_SERIES_INSTANCE_UIDS
        .iter()
        .map(|series_instance_uid| {
            thread::spawn(|| orthanc_store(ORTHANC_URL, AE_TITLE, series_instance_uid))
        })
        .map(|thread| thread.join().unwrap().unwrap())
        .map(|res| {
            assert_eq!(res.failed_instances_count, 0);
            res.instances_count
        })
        .sum();

    server_thread.join().unwrap();

    let client = reqwest::blocking::ClientBuilder::new()
        .use_rustls_tls()
        .build()
        .unwrap();
    let response: PacsFilesList = client
        .get(format!("{}pacsfiles/", cube_url))
        .basic_auth(username, Some(password))
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(response.count, total_dicom_instances)
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
    run_server_from_env(Some(n_clients), n_proc, Some(n_clients)).unwrap()
}

#[derive(serde::Deserialize)]
struct PacsFilesList {
    count: usize,
}
