//! File mostly copied from dicom-rs.
//!
//! https://github.com/Enet4/dicom-rs/blob/dbd41ed3a0d1536747c6b8ea2b286e4c6e8ccc8a/storescp/src/main.rs

use opentelemetry::global;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::TracerProvider;

use oxidicom::run_server_from_env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing().unwrap();
    let result = run_server_from_env(None, None);
    global::shutdown_tracer_provider();
    result
}

fn init_tracing() -> Result<(), opentelemetry::trace::TraceError> {
    global::set_text_map_propagator(TraceContextPropagator::new());
    let exporter = opentelemetry_otlp::new_exporter()
        .http()
        .build_span_exporter()?;
    let provider = TracerProvider::builder()
        .with_simple_exporter(exporter)
        .build();
    global::set_tracer_provider(provider);
    Ok(())
}
