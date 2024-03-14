use std::thread;

use camino::Utf8Path;

use oxidicom::run_server_from_env;

use crate::storescu::{dicom_client, get_test_files};

mod storescu;

const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");
const EXAMPLE_DATA_DIR: &str = "example_data";
const EXAMPLE_SAG_ANON: &str = "FNNDSC-SAG-anon-3d6e850";
const EXAMPLE_GREENEYES: &str = "greenEyes-anat";

/// Runs the DICOM listener and pushes 2 series to it in parallel.
#[test]
fn test_register_pacsfiles_to_cube() {
    let cube_url = envmnt::get_or_panic("CHRIS_URL");
    let username = envmnt::get_or_panic("CHRIS_USERNAME");
    let password = envmnt::get_or_panic("CHRIS_PASSWORD");

    let examples_dir = Utf8Path::new(CARGO_MANIFEST_DIR).join(EXAMPLE_DATA_DIR);
    let greeneyes_dir = examples_dir.join(EXAMPLE_GREENEYES);
    let sag_anon_dir = examples_dir.join(EXAMPLE_SAG_ANON);

    let server_thread = thread::spawn(|| run_server_for_test(2));
    let push_greeneyes = thread::spawn(|| dicom_client(greeneyes_dir));
    let push_sag_anon = thread::spawn(|| dicom_client(sag_anon_dir));

    push_greeneyes.join().unwrap();
    push_sag_anon.join().unwrap();
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
    assert_eq!(response.count, get_test_files(&examples_dir).len())
}

/// Create and run a server which will shut down after a given number of connections.
fn run_server_for_test(n_clients: usize) {
    run_server_from_env(Some(n_clients), Some(n_clients)).unwrap()
}

#[derive(serde::Deserialize)]
struct PacsFilesList {
    count: usize,
}
