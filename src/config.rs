use figment::providers::Env;
use figment::Figment;
use std::sync::OnceLock;

static CONFIG: OnceLock<Figment> = OnceLock::new();

pub fn get_config() -> &'static Figment {
    CONFIG.get_or_init(|| {
        Figment::new()
            .merge(Env::prefixed("OXIDICOM_").split("_"))
            .merge(Env::prefixed("OXIDICOM_"))
    })
}
