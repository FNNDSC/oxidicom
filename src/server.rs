use crate::scp::handle_incoming_dicom;
use crate::{ChrisPacsStorage, DicomRsConfig};
use std::net::{SocketAddrV4, TcpListener, TcpStream};
use std::sync::Arc;
use tracing::{error, info};
use crate::threads::ThreadPool;

/// `finite_connections` is a variable only used for testing. It tells the server to exit
/// after a finite number of connections, or on the first error.
pub fn run_server(
    address: &SocketAddrV4,
    chris: ChrisPacsStorage,
    options: DicomRsConfig,
    mut finite_connections: Option<usize>,
    n_threads: usize
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(address)?;
    info!("listening on: tcp://{}", address);

    let pool = ThreadPool::new(n_threads);
    let chris = Arc::new(chris);
    let options = Arc::new(options);

    for stream in listener.incoming() {
        match stream {
            Ok(scu_stream) => {
                let chris = Arc::clone(&chris);
                let options = Arc::clone(&options);
                pool.execute(move || {
                    if let Err(e) = handle_incoming_dicom(scu_stream, &chris, &options) {
                        error!("{}", snafu::Report::from_error(e));
                    }
                });
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
        // fixme
        finite_connections = finite_connections.map(|n| n - 1);
        if finite_connections.map(|n| n == 0).unwrap_or(false) {
            break;
        }
    }
    Ok(())
}
