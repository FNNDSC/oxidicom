use crate::scp::handle_incoming_dicom;
use crate::threads::ThreadPool;
use crate::{ChrisPacsStorage, DicomRsConfig};
use opentelemetry::trace::{Status, TraceContextExt, Tracer};
use opentelemetry::{global, KeyValue};
use std::net::{SocketAddrV4, TcpListener, TcpStream};
use std::sync::Arc;

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
    eprintln!("listening on: tcp://{}", address);

    let mut pool = ThreadPool::new(n_threads);
    let chris = Arc::new(chris);
    let options = Arc::new(options);

    let incoming: Box<dyn Iterator<Item = Result<TcpStream, _>>> =
        if let Some(n) = finite_connections {
            Box::new(listener.incoming().take(n))
        } else {
            Box::new(listener.incoming())
        };
    let tracer = global::tracer(env!("CARGO_PKG_NAME"));
    for stream in incoming {
        tracer.in_span("association", |cx| match stream {
            Ok(scu_stream) => {
                if let Ok(address) = scu_stream.peer_addr() {
                    cx.span()
                        .set_attribute(KeyValue::new("address", address.to_string()));
                }
                let chris = Arc::clone(&chris);
                let options = Arc::clone(&options);
                pool.execute(move || {
                    if let Err(e) = handle_incoming_dicom(scu_stream, &chris, &options) {
                        cx.span().set_status(Status::error(e.to_string()))
                    } else {
                        cx.span().set_status(Status::Ok)
                    }
                });
            }
            Err(e) => cx.span().set_status(Status::error(e.to_string())),
        })
    }
    pool.shutdown();
    Ok(())
}
