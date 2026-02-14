use camino::Utf8Path;
use oxidicom::{DicomRsSettings, OxidicomEnvOptions};
use std::num::NonZeroUsize;
use std::sync::Once;
use std::time::Duration;

static INIT_LOGGING: Once = Once::new();

pub(crate) fn sleep_duration() -> Duration {
    if matches!(option_env!("CI"), Some("true")) {
        Duration::from_secs(10)
    } else {
        Duration::from_millis(1500)
    }
}

pub(crate) fn init_logging() {
    INIT_LOGGING.call_once(|| {
        tracing::subscriber::set_global_default(
            tracing_subscriber::FmtSubscriber::builder()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .finish(),
        )
        .unwrap()
    })
}

pub(crate) fn create_test_options<P: AsRef<Utf8Path>>(
    files_root: P,
    root_subject: String,
    listener_port: u16,
) -> OxidicomEnvOptions {
    OxidicomEnvOptions {
        files_root: files_root.as_ref().to_path_buf(),
        nats_address: Some("localhost:4222".to_string()),
        progress_interval: Duration::from_millis(50),
        scp: DicomRsSettings {
            aet: "OXIDICOMTEST".to_string(),
            strict: false,
            uncompressed_only: false,
            promiscuous: true,
        },
        scp_max_pdu_length: 16384,
        listener_threads: NonZeroUsize::new(2).unwrap(),
        listener_port,
        dev_sleep: None,
        root_subject,
        cube_login_url: "http://localhost:8000/api/v1/auth-token/".to_string(),
        cube_chris_password: "chris1234".to_string(),
        cube_series_url: "http://localhost:8000/api/v1/pacs/series/".to_string(),
    }
}
