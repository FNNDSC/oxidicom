use crate::scp::handle_incoming_dicom;
use crate::{ChrisPacsStorage, DicomRsConfig};
use std::net::{SocketAddrV4, TcpListener};
use tracing::{error, info};

/// `finite_connections` is a variable only used for testing. It tells the server to exit
/// after a finite number of connections, or on the first error.
pub fn run_server(
    address: &SocketAddrV4,
    chris: ChrisPacsStorage,
    options: DicomRsConfig,
    mut finite_connections: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(address)?;
    info!("listening on: tcp://{}", address);

    for stream in listener.incoming() {
        match stream {
            Ok(scu_stream) => {
                if let Err(e) = handle_incoming_dicom(scu_stream, &chris, &options) {
                    error!("{}", snafu::Report::from_error(e));
                }
            }
            Err(e) => {
                if finite_connections.is_some() {
                    error!("{}", snafu::Report::from_error(&e));
                    return Err(Box::new(e));
                } else {
                    error!("{}", snafu::Report::from_error(&e));
                }
            }
        }
        finite_connections = finite_connections.map(|n| n - 1);
        if finite_connections.map(|n| n == 0).unwrap_or(false) {
            break;
        }
    }
    Ok(())
}
