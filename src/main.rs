//! Initialize OpenTelemetry, then call [oxidicom::run_everything_from_env].

use oxidicom::get_config;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    init_tracing_subscriber().unwrap();
    init_otel_tracing().unwrap();
    let result = oxidicom::run_everything_from_env(None).await;
    opentelemetry::global::shutdown_tracer_provider();
    result
}

fn init_otel_tracing() -> Result<opentelemetry_sdk::trace::Tracer, opentelemetry::trace::TraceError>
{
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(opentelemetry_otlp::new_exporter().tonic())
        .install_batch(opentelemetry_sdk::runtime::Tokio)
}

fn init_tracing_subscriber() -> Result<(), tracing::dispatcher::SetGlobalDefaultError> {
    let level = if get_config().extract_inner_lossy("verbose").unwrap_or(false) {
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
