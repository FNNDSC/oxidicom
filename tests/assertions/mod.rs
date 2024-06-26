use chris::types::{CubeUrl, Username};
use chris::ChrisClient;
use figment::providers::Env;
use figment::Figment;

use crate::{CALLED_AE_TITLE, EXAMPLE_SERIES_INSTANCE_UIDS};

pub async fn run_assertions(expected_counts: &[usize]) {
    let client = get_client_from_env().await;
    for (series, expected_count) in EXAMPLE_SERIES_INSTANCE_UIDS
        .iter()
        .zip(expected_counts.into_iter())
    {
        let actual_count = client
            .pacsfiles()
            .series_instance_uid(*series)
            .pacs_identifier(CALLED_AE_TITLE)
            .search()
            .get_count()
            .await
            .unwrap();
        assert_eq!(actual_count, *expected_count);

        // the "Oxidicom Custom Metadata" spec should store the NumberOfSeriesRelatedInstances
        // in a blank file with the filename NumberOfSeriesRelatedInstances=value,
        // and searchable by ProtocolName.
        let custom_file_num_related = client.pacsfiles()
            .pacs_identifier(oxidicom::OXIDICOM_CUSTOM_PACS_NAME)
            .series_instance_uid(*series)
            .protocol_name("NumberOfSeriesRelatedInstances")
            .search()
            .get_first()
            .await
            .unwrap()
            .expect("\"Oxidicom Custom Metadata\" file for NumberOfSeriesRelatedInstances not found. Usually, it should be registered before all DICOM instances are done being registered.")
            .object;

        // The value should be stored as the SeriesDescription
        let actual_value = custom_file_num_related.series_description;
        let expected_value = Some(expected_count.to_string());
        assert_eq!(actual_value, expected_value);

        let actual_basename = custom_file_num_related
            .fname
            .as_str()
            .rsplit_once('/')
            .map(|(_l, r)| r)
            .unwrap_or(custom_file_num_related.fname.as_str());
        let expected_basename = format!("NumberOfSeriesRelatedInstances={expected_count}");
        assert_eq!(actual_basename, &expected_basename);

        // the "Oxidicom Custom Metadata" spec should store the OxidicomAttemptedPushCount
        // in a blank file with the filename OxidicomAttemptedPushCount=value,
        // and searchable by ProtocolName.
        let custom_file_num_attempts = client.pacsfiles()
            .pacs_identifier(oxidicom::OXIDICOM_CUSTOM_PACS_NAME)
            .series_instance_uid(*series)
            .protocol_name("OxidicomAttemptedPushCount")
            .search()
            .get_first()
            .await
            .unwrap()
            .expect("\"Oxidicom Custom Metadata\" file for OxidicomAttemptedPushCount not found. It should be registered after the last DICOM file was pushed.")
            .object;
        assert_eq!(
            custom_file_num_attempts.series_description,
            Some(expected_count.to_string())
        )
    }
}

async fn get_client_from_env() -> ChrisClient {
    let TestSettings {
        url,
        username,
        password,
    } = Figment::from(Env::prefixed("OXIDICOM_TEST_"))
        .extract()
        .unwrap();

    let account = chris::Account::new(&url, &username, &password);
    let token = account.get_token().await.unwrap();
    ChrisClient::build(url, username, token)
        .unwrap()
        .connect()
        .await
        .unwrap()
}

#[derive(serde::Deserialize)]
struct TestSettings {
    url: CubeUrl,
    username: Username,
    password: String,
}
