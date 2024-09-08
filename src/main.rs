//! Initialize OpenTelemetry, then call [oxidicom::run_everything_from_env].

use figment::providers::Env;
use figment::Figment;
use opentelemetry::trace::TraceError;
use opentelemetry_sdk::trace::TracerProvider;
use std::sync::LazyLock;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    init_tracing_subscriber()?;
    init_otel_tracing()?;
    let result = run_everything_from_env(None).await;
    opentelemetry::global::shutdown_tracer_provider();
    result
}

/// Calls [run_everything] using configuration from environment variables.
///
/// Function parameters are prioritized over environment variable values.
///
/// `finite_connections`: shut down the server after the given number of DICOM associations.
pub async fn run_everything_from_env(finite_connections: Option<usize>) -> anyhow::Result<()> {
    let settings = CONFIG.extract()?;
    let on_start = |addr| tracing::info!("listening on: tcp://{}", addr);
    oxidicom::run_everything(settings, finite_connections, Some(on_start)).await
}

fn init_otel_tracing() -> Result<TracerProvider, TraceError> {
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(opentelemetry_otlp::new_exporter().tonic())
        .install_batch(opentelemetry_sdk::runtime::Tokio)
}

fn init_tracing_subscriber() -> Result<(), tracing::dispatcher::SetGlobalDefaultError> {
    let level = if CONFIG.extract_inner_lossy("verbose").unwrap_or(false) {
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

static CONFIG: LazyLock<Figment> = LazyLock::new(|| {
    Figment::new()
        .merge(Env::prefixed("OXIDICOM_").split("_"))
        .merge(Env::prefixed("OXIDICOM_"))
});
