//! Initialize OpenTelemetry, then call [run_server_from_env].

use opentelemetry::global;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::TracerProvider;

use oxidicom::run_server_from_env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing_subscriber().unwrap();
    init_otel_tracing().unwrap();
    let result = run_server_from_env(None, None);
    global::shutdown_tracer_provider();
    result
}

fn init_otel_tracing() -> Result<(), opentelemetry::trace::TraceError> {
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

fn init_tracing_subscriber() -> Result<(), tracing::dispatcher::SetGlobalDefaultError> {
    let verbose_option = envmnt::get_or("CHRIS_VERBOSE", "");
    let level = if verbose_option.to_lowercase().starts_with("y") {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(level)
            .finish(),
    )
}