use crate::scp::handle_incoming_dicom;
use crate::threads::ThreadPool;
use crate::{ChrisPacsStorage, DicomRsConfig};
use std::net::{SocketAddrV4, TcpListener, TcpStream};
use std::sync::Arc;
use tracing::{error, info};

/// `finite_connections` is a variable only used for testing. It tells the server to exit
/// after a finite number of connections, or on the first error.
pub fn run_server(
    address: &SocketAddrV4,
    chris: ChrisPacsStorage,
    options: DicomRsConfig,
    finite_connections: Option<usize>,
    n_threads: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(address)?;
    info!("listening on: tcp://{}", address);

    let mut pool = ThreadPool::new(n_threads);
    let chris = Arc::new(chris);
    let options = Arc::new(options);

    let incoming: Box<dyn Iterator<Item = Result<TcpStream, _>>> =
        if let Some(n) = finite_connections {
            Box::new(listener.incoming().take(n))
        } else {
            Box::new(listener.incoming())
        };

    for stream in incoming {
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
                } else {
                    error!("{}", snafu::Report::from_error(&e));
                }
            }
        }
    }
    pool.shutdown();
    Ok(())
}
