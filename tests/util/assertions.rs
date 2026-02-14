pub use crate::util::expected::EXPECTED_SERIES;
use async_walkdir::WalkDir;
use camino::{Utf8Path, Utf8PathBuf};
use futures::TryStreamExt;
use oxidicom::cube_publisher::get_chris_token;
use oxidicom::types::CollectionJSON;
use oxidicom::{AETitle, SeriesKey};
use reqwest::header::AUTHORIZATION;

pub const ROOT_SUBJECT: &str = "test.oxidicom";

pub async fn assert_files_stored(storage_path: &Utf8Path) {
    let (expected, actual) = tokio::join!(expected_files(), find_files(storage_path));
    pretty_assertions::assert_eq!(expected, actual)
}

pub async fn assert_cube_record(series_instance_uid: String) {
    let client = reqwest::Client::new();

    let cube_login_url = "http://localhost:8000/api/v1/auth-token/".to_string();
    let cube_chris_password = "chris1234".to_string();
    let cube_chris_token = get_chris_token(&client, cube_login_url, cube_chris_password).await;

    let cube_series_url = format!(
        "http://localhost:8000/api/v1/pacs/series/search/?SeriesInstanceUID={series_instance_uid}"
    )
    .to_string();

    let res = client
        .get(&cube_series_url)
        .header(AUTHORIZATION, format!("Token {cube_chris_token}"))
        .send()
        .await;

    match res {
        Ok(r) => {
            let collection_json = r.json::<CollectionJSON>().await;
            match collection_json {
                Ok(collection_res) => {
                    tracing::info!(
                        msg = format!("collection_json: {x}", x = collection_res.collection.total),
                    );
                    assert_eq!(collection_res.collection.total, 1);
                }
                Err(e) => {
                    tracing::error!(msg = format!("unable to r.json: e: {e}"));
                    assert!(false);
                }
            }
        }
        Err(e) => {
            tracing::error!(msg = format!("unable to client.send: e: {e}"));
            assert!(false);
        }
    }
}

async fn expected_files() -> Vec<String> {
    let path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("expected_files.txt");
    let content = tokio::fs::read_to_string(path).await.unwrap();
    content
        .split("\n")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

async fn find_files(storage_path: &Utf8Path) -> Vec<String> {
    let mut files: Vec<String> = WalkDir::new(storage_path)
        .try_filter_map(|entry| async move {
            if entry.file_type().await.unwrap().is_file() {
                let path = Utf8PathBuf::from_path_buf(entry.path())
                    .map(|p| pathdiff::diff_utf8_paths(p, storage_path).unwrap())
                    .map(|p| p.into_string())
                    .expect("Invalid UTF-8 path found");
                Ok(Some(path))
            } else {
                Ok(None)
            }
        })
        .try_collect()
        .await
        .unwrap();
    files.sort();
    files
}

pub fn assert_lonk_messages(messages: Vec<async_nats::Message>) {
    for series in &*EXPECTED_SERIES {
        let series_key = SeriesKey {
            SeriesInstanceUID: series.SeriesInstanceUID.to_string(),
            pacs_name: AETitle::from(series.pacs_name.as_str()),
            association: ulid::Ulid::new(),
        };
        let subject = oxidicom::lonk::subject_of(&ROOT_SUBJECT, &series_key);
        let messages_of_series: Vec<_> = messages
            .iter()
            .filter(|message| message.subject.as_str() == &subject)
            .collect();
        assert_messages_for_series(&messages_of_series, series.ndicom as u32)
    }
}

fn assert_messages_for_series(messages: &[&async_nats::Message], expected_ndicom: u32) {
    tracing::debug!(
        "Received data from NATS:\n---\n{}\n---",
        messages
            .iter()
            .map(|message| &message.payload)
            .map(|payload| {
                payload
                    .iter()
                    .map(|b| format!("{b:#04x}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n")
    );

    assert!(
        messages.len() >= 3,
        "There must be at least 3 messages per series: (1) first progress message, \
        (2) last progress message, (3) done message"
    );

    let mut prev = 0;
    for message in &messages[..messages.len() - 2] {
        let payload = &message.payload;
        let first_byte = *payload.first().unwrap();
        assert_eq!(first_byte, oxidicom::lonk::MESSAGE_NDICOM);
        assert_eq!(payload.len(), 1 + size_of::<u32>());
        let num = u32::from_le_bytes([payload[1], payload[2], payload[3], payload[4]]);
        assert!(
            num > prev,
            "ndicom progress message value must always increase."
        );
        prev = num;
    }

    let second_last = &messages[messages.len() - 2].payload;
    assert_eq!(second_last[0], oxidicom::lonk::MESSAGE_NDICOM);
    let last_ndicom = u32::from_le_bytes([
        second_last[1],
        second_last[2],
        second_last[3],
        second_last[4],
    ]);
    assert_eq!(last_ndicom, expected_ndicom);

    assert_eq!(
        messages.last().unwrap().payload,
        oxidicom::lonk::done_message()
    );
}
