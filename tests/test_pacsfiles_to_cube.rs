use std::net::{Ipv4Addr, SocketAddrV4};
use std::thread;

use camino::{Utf8Path, Utf8PathBuf};

use oxidicom::{run_server, ChrisPacsStorage, DicomRsConfig};

use crate::storescu::{dicom_client, get_test_files};

mod storescu;

const CHRIS_PACSFILES_URL: &str = "http://chris:8000/api/v1/pacsfiles/";
const CHRIS_USERNAME: &str = "chris";
const CHRIS_PASSWORD: &str = "chris1234";
const CHRIS_FILES_ROOT: &str = "/data";
const CARGO_MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");
const EXAMPLE_DATA_DIR: &str = "example_data";
const EXAMPLE_SAG_ANON: &str = "FNNDSC-SAG-anon-3d6e850";
const EXAMPLE_GREENEYES: &str = "greenEyes-anat";

/// Runs the DICOM listener and pushes 2 series to it in parallel.
#[test]
fn test_register_pacsfiles_to_cube() {
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(tracing::Level::INFO)
            .finish(),
    )
    .unwrap();

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
        .get(CHRIS_PACSFILES_URL)
        .basic_auth(CHRIS_USERNAME, Some(CHRIS_PASSWORD))
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .unwrap()
        .json()
        .unwrap();
    assert_eq!(response.count, get_test_files(&examples_dir).len())
}


/// Create and run a server which will shut down after a given number of connections.
fn run_server_for_test(n_clients: usize) {
    let address = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 11112);
    let chris = ChrisPacsStorage::new(
        CHRIS_PACSFILES_URL.to_string(),
        CHRIS_USERNAME.to_string(),
        CHRIS_PASSWORD.to_string(),
        Utf8PathBuf::from(CHRIS_FILES_ROOT),
        3,
        Some("THETESTPACS".to_string()),
    );
    let options = DicomRsConfig {
        calling_ae_title: "ChRISTEST".to_string(),
        strict: false,
        uncompressed_only: false,
        max_pdu_length: 16384,
    };
    run_server(&address, chris, options, Some(n_clients), n_clients).unwrap()
}

#[derive(serde::Deserialize)]
struct PacsFilesList {
    count: usize,
}
