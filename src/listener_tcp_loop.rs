use crate::dicomrs_options::{ClientAETitle, DicomRsConfig};
use crate::event::AssociationEvent;
use crate::scp::handle_association;
use crate::thread_pool::ThreadPool;
use opentelemetry::trace::{Status, TraceContextExt, Tracer};
use opentelemetry::{global, Context, KeyValue};
use opentelemetry_semantic_conventions as semconv;
use std::collections::HashMap;
use std::net::{SocketAddrV4, TcpListener, TcpStream};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// Listen for incoming DICOM instances on a TCP port.
///
/// Every TCP connection is handled by [handle_association], which transmits DICOM instance file
/// objects through the given `handler`.
pub fn dicom_listener_tcp_loop(
    address: SocketAddrV4,
    config: DicomRsConfig,
    finite_connections: Option<usize>,
    n_threads: usize,
    max_pdu_length: usize,
    handler: UnboundedSender<AssociationEvent>,
    pacs_addresses: HashMap<ClientAETitle, String>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(address)?;
    tracing::info!("listening on: tcp://{}", address);
    let mut pool = ThreadPool::new(n_threads, "dicom_listener");
    let ae_title = Arc::new(config.aet.clone());
    let pacs_addresses = Arc::new(pacs_addresses);
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
                let ae_title = Arc::clone(&ae_title);
                let pacs_address = Arc::clone(&pacs_addresses);
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
                    match handle_association(
                        scu_stream,
                        &options,
                        max_pdu_length,
                        &handler,
                        ulid,
                        &ae_title,
                        &pacs_address,
                    ) {
                        Ok(..) => {
                            handler
                                .send(AssociationEvent::Finish {
                                    ulid,
                                    ok: true,
                                })
                                .unwrap();
                            context.span().set_status(Status::Ok)
                        }
                        Err(e) => {
                            tracing::error!("{:?}", e);
                            handler
                                .send(AssociationEvent::Finish {
                                    ulid,
                                    ok: false,
                                })
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


// /// Start listening for DICOM instances.
// ///
// /// This function creates 2 thread pools:
// ///
// /// - Receive DICOM files from a TCP port over the DIMSE C-STORE protocol
// /// - Push DICOM files to _CUBE_
// ///
// /// `finite_connections` is a variable only used for testing. It tells the server to exit
// /// after a finite number of connections, or on the first error.
// pub fn run_dicom_listener(
//     address: SocketAddrV4,
//     config: DicomRsConfig,
//     pacs_addresses: HashMap<ClientAETitle, String>,
//     max_pdu_length: usize,
//     finite_connections: Option<usize>,
//     listener_threads: usize,
//     pusher_threads: usize,
// ) -> anyhow::Result<()> {
//     let (rx, tx) = mpsc::channel();
//     let pusher = thread::spawn(move || run_chris_pusher(chris, tx, pusher_threads));
//     let listener = thread::spawn(move || {
//         dicom_listener_tcp_loop(
//             address,
//             config,
//             finite_connections,
//             listener_threads,
//             max_pdu_length,
//             rx,
//             pacs_addresses,
//         )
//     });
//     listener.join().unwrap()?;
//     pusher.join().unwrap()
// }
//
// /// Wait for received DICOM instances. For each DICOM instance file, push and register it to _CUBE_.
// fn run_chris_pusher(
//     client: CubePacsStorageClient,
//     incoming: Receiver<AssociationEvent>,
//     n_threads: usize,
// ) -> anyhow::Result<()> {
//     let mut pool = ThreadPool::new(n_threads, "cube_pusher");
//     let sender = ChrisSender::new(client);
//     let had_error = Arc::new(Mutex::new(false));
//     while let Ok(event) = incoming.recv() {
//         let jobs = sender.prepare_jobs_for(event);
//         for job in jobs {
//             let has_error = Arc::clone(&had_error);
//             pool.execute(move || {
//                 if let Err(e) = job.run() {
//                     tracing::error!("{}", e.to_string());
//                     *has_error.lock().unwrap() = true;
//                 }
//             });
//         }
//     }
//     pool.shutdown();
//     if Arc::into_inner(had_error).unwrap().into_inner().unwrap() {
//         anyhow::bail!("Some pushes to CUBE were unsuccessful")
//     } else {
//         Ok(())
//     }
// }