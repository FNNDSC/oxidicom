//! Initialize OpenTelemetry, then call [oxidicom::run_everything_from_env].

use figment::providers::Env;
use figment::Figment;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::sync::LazyLock;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    init_tracing_subscriber()?;
    let provider = init_tracer_provider()?;
    let result = run_everything_from_env(None).await;
    provider.shutdown()?;
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

fn init_tracer_provider() -> Result<SdkTracerProvider, opentelemetry::trace::TraceError> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .build()?;
    let resource = opentelemetry_sdk::Resource::builder().build();
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();
    Ok(provider)
}

fn init_tracing_subscriber() -> Result<(), tracing::dispatcher::SetGlobalDefaultError> {
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .finish(),
    )
}

static CONFIG: LazyLock<Figment> = LazyLock::new(|| {
    Figment::new()
        .merge(Env::prefixed("OXIDICOM_").split("_"))
        .merge(Env::prefixed("OXIDICOM_"))
});
