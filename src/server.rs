use crate::dicomrs_options::DicomRsConfig;
use crate::scp::handle_incoming_dicom;
use crate::threads::ThreadPool;
use crate::ChrisPacsStorage;
use opentelemetry::trace::{Status, TraceContextExt, Tracer};
use opentelemetry::{global, Context, KeyValue};
use opentelemetry_semantic_conventions as semconv;
use std::net::{SocketAddrV4, TcpListener, TcpStream};
use std::sync::Arc;

/// `finite_connections` is a variable only used for testing. It tells the server to exit
/// after a finite number of connections, or on the first error.
pub fn run_server(
    address: &SocketAddrV4,
    chris: ChrisPacsStorage,
    config: DicomRsConfig,
    finite_connections: Option<usize>,
    n_threads: usize,
    max_pdu_length: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(address)?;
    tracing::info!("listening on: tcp://{}", address);

    let mut pool = ThreadPool::new(n_threads);
    let chris = Arc::new(chris);
    let options = Arc::new(config.into());

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
                let chris = Arc::clone(&chris);
                let options = Arc::clone(&options);
                pool.execute(move || {
                    let _context_guard = cx.attach();
                    let context = Context::current();
                    let mut peer_address = None;
                    if let Ok(address) = scu_stream.peer_addr() {
                        let peer_attributes = vec![
                            KeyValue::new(semconv::trace::CLIENT_ADDRESS, address.ip().to_string()),
                            KeyValue::new(semconv::trace::CLIENT_PORT, address.port() as i64),
                        ];
                        context.span().set_attributes(peer_attributes);
                        peer_address = Some(address);
                    }
                    match handle_incoming_dicom(scu_stream, &chris, &options, max_pdu_length) {
                        Ok(count) => {
                            if count == 0 {
                                tracing::warn!("Did not receive any files from {:?}", peer_address);
                            } else {
                                context.span().set_status(Status::Ok)
                            }
                        }
                        Err(e) => {
                            tracing::error!("{:?}", e);
                            context.span().set_status(Status::error(e.to_string()))
                        }
                    }
                });
            }
            Err(e) => cx.span().set_status(Status::error(e.to_string())),
        })
    }
    pool.shutdown();
    Ok(())
}
