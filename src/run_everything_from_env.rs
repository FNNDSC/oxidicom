use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4};

use camino::Utf8PathBuf;
use sqlx::postgres::PgPoolOptions;

use crate::chrisdb_client::CubePostgresClient;
use crate::dicomrs_options::{ClientAETitle, OurAETitle};
use crate::run_everything::run_everything;
use crate::DicomRsConfig;

/// Calls [run_dicom_listener] using configuration from environment variables.
///
/// Function parameters are prioritized over environment variable values.
///
/// `finite_connections`: shut down the server after the given number of DICOM associations.
pub async fn run_everything_from_env(finite_connections: Option<usize>) -> anyhow::Result<()> {
    // TODO replace envmnt with https://docs.rs/config/0.14.0/config/
    // let config = config::Config::builder().add_source(config::Environment::with_prefix("OXIDICOM").separator("_")).build()?;
    // let files_root = config.get_string("FILES_ROOT").map(Utf8PathBuf::from)?;

    // let port = config.get("PORT").unwrap_or(11111);
    let files_root = Utf8PathBuf::from(envmnt::get_or_panic("OXIDICOM_FILES_ROOT"));
    let address = SocketAddrV4::new(Ipv4Addr::from(0), envmnt::get_u16("OXIDICOM_PORT", 11111));

    let dicomrs_config = DicomRsConfig {
        aet: OurAETitle::from(envmnt::get_or("OXIDICOM_SCP_AET", "ChRIS")),
        strict: envmnt::is_or("OXIDICOM_SCP_STRICT", false),
        uncompressed_only: envmnt::is_or("OXIDICOM_SCP_UNCOMPRESSED_ONLY", false),
        promiscuous: true,
    };

    let pacs_addresses = parse_string_dict(envmnt::get_or("OXIDICOM_PACS_ADDRESS", ""))?;
    let listener_threads = envmnt::get_usize("OXIDICOM_LISTENER_THREADS", 16);
    let max_pdu_length = envmnt::get_usize("OXIDICOM_SCP_MAX_PDU_LENGTH", 16384);

    // let db_connection = config.get_string("OXIDICOM_DB_CONNECTION")?;
    let db_connection = envmnt::get_or_panic("OXIDICOM_DB_CONNECTION");
    let db_pool_size = envmnt::get_u32("OXIDICOM_DB_POOL", 10);
    let db_batch_size = envmnt::get_usize("OXIDICOM_DB_BATCH_SIZE", 20);

    let db_pool = PgPoolOptions::new()
        .max_connections(db_pool_size)
        .connect(&db_connection)
        .await?;
    let cubedb_client = CubePostgresClient::new(db_pool, None);

    run_everything(
        address,
        dicomrs_config,
        pacs_addresses,
        max_pdu_length,
        finite_connections,
        listener_threads,
        files_root,
        cubedb_client,
        db_batch_size,
    )
    .await
}

fn parse_string_dict(s: impl AsRef<str>) -> anyhow::Result<HashMap<ClientAETitle, String>> {
    s.as_ref()
        .split(',')
        .filter_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .map(parse_key_value_pair)
        .collect()
}

fn parse_key_value_pair(s: &str) -> anyhow::Result<(ClientAETitle, String)> {
    s.split_once('=')
        .map(|(l, r)| (ClientAETitle::from(l), r.to_string()))
        .ok_or_else(|| {
            anyhow::Error::msg(format!(
                "Bad value for OXIDICOM_PACS_ADDRESS: \"{s}\" does not contain a '='"
            ))
        })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rstest::*;

    use crate::dicomrs_options::ClientAETitle;

    use super::parse_string_dict;

    #[rstest]
    #[case("", [])]
    #[case("BCH=1.2.3.4:4242", [("BCH", "1.2.3.4:4242")])]
    #[case("BCH=1.2.3.4:4242,", [("BCH", "1.2.3.4:4242")])]
    #[case("BCH=1.2.3.4:4242,MGH=5.6.7.8:9090", [("BCH", "1.2.3.4:4242"), ("MGH", "5.6.7.8:9090")])]
    fn test_parse_string_dict(
        #[case] given: &str,
        #[case] expected: impl IntoIterator<Item = (&'static str, &'static str)>,
    ) {
        let expected: HashMap<_, _> = expected
            .into_iter()
            .map(|(aec, addr)| (ClientAETitle::from_static(aec), addr.to_string()))
            .collect();
        let actual = parse_string_dict(given).unwrap();
        assert_eq!(actual, expected)
    }
}
