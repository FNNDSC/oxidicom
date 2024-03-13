use std::net::{SocketAddrV4, TcpListener};
use tracing::{error, info};
use crate::{ChrisPacsStorage, DicomRsConfig};
use crate::scp::handle_incoming_dicom;

pub fn run_server(address: &SocketAddrV4, chris: ChrisPacsStorage, options: DicomRsConfig) -> Result<(), Box<dyn std::error::Error>> {
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
                error!("{}", snafu::Report::from_error(e));
            }
        }
    }

    Ok(())
}
