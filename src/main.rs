//! Initialize OpenTelemetry, then call [run_server_from_env].

use opentelemetry::global;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::TracerProvider;

use oxidicom::run_server_from_env;

fn main() -> anyhow::Result<()> {
    init_tracing_subscriber().unwrap();
    init_otel_tracing().unwrap();
    let result = run_server_from_env(None, None, None);
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
    let verbose_option = envmnt::get_or("CHRIS_VERBOSE", "").to_lowercase();
    let level = if verbose_option.starts_with("y") || &verbose_option == "true" {
        tracing::Level::INFO
    } else {
        tracing::Level::WARN
    };
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(level)
            .finish(),
    )
}
