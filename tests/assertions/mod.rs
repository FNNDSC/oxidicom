mod expected;
mod model;

use crate::assertions::model::SeriesParams;
use async_walkdir::WalkDir;
use camino::{Utf8Path, Utf8PathBuf};
use celery::broker::{AMQPBrokerBuilder, BrokerBuilder};
use celery::prelude::BrokerError;
use celery::protocol::MessageBody;
pub use expected::EXPECTED_SERIES;
use futures::{stream, StreamExt, TryStreamExt};
use oxidicom::register_pacs_series;
use std::collections::HashSet;

pub async fn assert_files_stored(storage_path: &Utf8Path) {
    let (expected, actual) = tokio::join!(expected_files(), find_files(storage_path));
    pretty_assertions::assert_eq!(expected, actual)
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

pub async fn assert_rabbitmq_messages(address: &str, queue_name: &str) {
    let broker = Box::new(AMQPBrokerBuilder::new(address))
        .declare_queue(queue_name)
        .build(1000)
        .await
        .unwrap();
    let error_handler = Box::new(move |e: BrokerError| panic!("{:?}", e));
    let (_consumer_tag, consumer) = broker.consume(queue_name, error_handler).await.unwrap();

    // Deserialize deliveries into messages
    let messages_stream = consumer.try_filter_map(|delivery| async move {
        delivery.ack().await.unwrap();
        let body = delivery
            .try_deserialize_message()
            .and_then(|m| m.body::<register_pacs_series>())
            .unwrap();
        Ok(Some(body))
    });

    // Read the expected number of messages from the stream
    let params: HashSet<SeriesParams> = stream::iter(0..EXPECTED_SERIES.len())
        .zip(messages_stream)
        .map(|(_, r)| r)
        .try_filter_map(|r| async { Ok(Some(deserialize_params(r))) })
        .try_collect()
        .await
        .map_err(|e| format!("{}", e))
        .unwrap();

    assert_eq!(&*EXPECTED_SERIES, &params);
}

fn deserialize_params<T: celery::task::Task, D: serde::de::DeserializeOwned>(
    body: MessageBody<T>,
) -> D {
    if let serde_json::Value::Array(v) = serde_json::to_value(body).unwrap() {
        return v
            .into_iter()
            .map(serde_json::from_value)
            .filter_map(|r| r.ok())
            .next()
            .expect("No elements were deserializable to the specified type.");
    }
    panic!("Expected body to be an array, but it is not.")
}

pub async fn assert_lonk_messages(messages: Vec<async_nats::Message>) {
    println!("DUMPING MESSAGES");
    for message in messages {
        let hex = message
            .payload
            .iter()
            .map(|b| format!("{b:#04x}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!("{} <-- {}", message.subject, hex);
    }
    println!("DUMPING MESSAGES FINISH");
}
