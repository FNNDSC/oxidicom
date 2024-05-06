use crate::{CALLED_AE_TITLE, EXAMPLE_SERIES_INSTANCE_UIDS};
use chris::types::{CubeUrl, Username};
use chris::ChrisClient;
use std::time::Duration;

pub fn run_assertions(expected_counts: &[usize]) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(assertions(expected_counts))
}

async fn assertions(expected_counts: &[usize]) {
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

        // It might take "Oxidicom Custom Metadata" files a little bit more time to appear
        // after all DICOM files were pushed. So we sleep for 1 second, but no more.
        // (If they don't appear within 1 second, the performance is too bad, and we will
        // fail this test.)
        tokio::time::sleep(Duration::from_secs(1)).await;

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
    let cube_url = CubeUrl::new(envmnt::get_or_panic("CHRIS_URL")).unwrap();
    let username = Username::new(envmnt::get_or_panic("CHRIS_USERNAME"));
    let password = envmnt::get_or_panic("CHRIS_PASSWORD");
    let account = chris::Account::new(&cube_url, &username, &password);
    let token = account.get_token().await.unwrap();
    ChrisClient::build(cube_url, username, token)
        .unwrap()
        .connect()
        .await
        .unwrap()
}
