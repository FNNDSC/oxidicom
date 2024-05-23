use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4};

use crate::dicomrs_options::{ClientAETitle, OurAETitle};
use crate::run_everything::run_everything;
use crate::DicomRsConfig;

/// Calls [run_dicom_listener] using configuration from environment variables.
///
/// Function parameters are prioritized over environment variable values.
///
/// `finite_connections`: shut down the server after the given number of DICOM associations.
pub async fn run_everything_from_env(finite_connections: Option<usize>) -> anyhow::Result<()> {
    let address = SocketAddrV4::new(Ipv4Addr::from(0), envmnt::get_u16("PORT", 11111));
    let dicomrs_config = DicomRsConfig {
        aet: OurAETitle::from(envmnt::get_or("CHRIS_SCP_AET", "ChRIS")),
        strict: envmnt::is_or("CHRIS_SCP_STRICT", false),
        uncompressed_only: envmnt::is_or("CHRIS_SCP_UNCOMPRESSED_ONLY", false),
        promiscuous: true
    };

    let pacs_addresses = parse_string_dict(envmnt::get_or("CHRIS_PACS_ADDRESS", ""))?;
    let listener_threads = envmnt::get_usize("CHRIS_LISTENER_THREADS", 16);
    // let pusher_threads = envmnt::get_usize("CHRIS_PUSHER_THREADS", 8);
    let max_pdu_length = envmnt::get_usize("CHRIS_SCP_MAX_PDU_LENGTH", 16384);
    run_everything(
        address,
        dicomrs_config,
        pacs_addresses,
        max_pdu_length,
        finite_connections,
        listener_threads,
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
                "Bad value for CHRIS_PACS_ADDRESS: \"{s}\" does not contain a '='"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::parse_string_dict;
    use crate::dicomrs_options::ClientAETitle;
    use rstest::*;
    use std::collections::HashMap;

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
