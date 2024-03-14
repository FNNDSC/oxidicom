//! Initialize OpenTelemetry, then call [run_server_from_env].

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
