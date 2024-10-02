use crate::dicomrs_settings::DicomRsSettings;
use crate::enums::AssociationEvent;
use crate::scp::handle_association;
use crate::thread_pool::ThreadPool;
use opentelemetry::trace::{Status, TraceContextExt, Tracer};
use opentelemetry::{global, Context, KeyValue};
use opentelemetry_semantic_conventions as semconv;
use std::net::{SocketAddrV4, TcpListener, TcpStream};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// Listen for incoming DICOM instances on a TCP port.
///
/// Every TCP connection is handled by [handle_association], which transmits DICOM instance file
/// objects through the given `handler`.
pub fn dicom_listener_tcp_loop<F>(
    address: SocketAddrV4,
    config: DicomRsSettings,
    finite_connections: Option<usize>,
    n_threads: usize,
    max_pdu_length: usize,
    handler: UnboundedSender<AssociationEvent>,
    on_start: Option<F>,
) -> anyhow::Result<()>
where
    F: FnOnce(SocketAddrV4),
{
    let listener = TcpListener::bind(address)?;
    if let Some(f) = on_start {
        f(address)
    };

    let mut pool = ThreadPool::new(n_threads, "dicom_listener");
    let options = Arc::new(config.into());
    let handler = Arc::new(handler);
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
                let options = Arc::clone(&options);
                let handler = Arc::clone(&handler);
                pool.execute(move || {
                    let ulid = ulid::Ulid::new();
                    let _context_guard = cx.attach();
                    let context = Context::current();
                    let association_attribute = KeyValue::new("association_ulid", ulid.to_string());
                    context.span().set_attribute(association_attribute);
                    if let Ok(address) = scu_stream.peer_addr() {
                        let peer_attributes = vec![
                            KeyValue::new(semconv::trace::CLIENT_ADDRESS, address.ip().to_string()),
                            KeyValue::new(semconv::trace::CLIENT_PORT, address.port() as i64),
                        ];
                        context.span().set_attributes(peer_attributes);
                    }
                    match handle_association(scu_stream, &options, max_pdu_length, &handler, ulid) {
                        Ok(..) => {
                            handler
                                .send(AssociationEvent::Finish { ulid, ok: true })
                                .unwrap();
                            context.span().set_status(Status::Ok)
                        }
                        Err(e) => {
                            tracing::error!("{:?}", e);
                            handler
                                .send(AssociationEvent::Finish { ulid, ok: false })
                                .unwrap();
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
