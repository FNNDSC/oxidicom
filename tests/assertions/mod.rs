mod expected;
mod model;

pub use expected::EXPECTED_SERIES;
use std::collections::HashSet;

use crate::assertions::model::SeriesParams;
use camino::Utf8Path;
use celery::broker::{AMQPBrokerBuilder, BrokerBuilder};
use celery::prelude::BrokerError;
use celery::protocol::MessageBody;
use futures::{stream, StreamExt, TryStreamExt};
use oxidicom::register_pacs_series;

pub async fn assert_files_stored(storage_path: &Utf8Path) {
    stream::iter(&*EXPECTED_SERIES)
        .for_each_concurrent(EXPECTED_SERIES.len(), |series| {
            assert_series_path(storage_path, series)
        })
        .await;
}

async fn assert_series_path(storage_path: &Utf8Path, series: &SeriesParams) {
    let series_dir = storage_path.join(&series.path);
    let count = tokio::fs::read_dir(series_dir)
        .await
        .map(tokio_stream::wrappers::ReadDirStream::new)
        .unwrap()
        .map(|result| async {
            let entry = result.unwrap();
            assert!(
                entry.file_type().await.unwrap().is_file(),
                "{:?} is not a file. PACSSeries folder may only contain files.",
                entry.path()
            );
            assert_eq!(
                entry
                    .path()
                    .extension()
                    .expect("Found file without file extension")
                    .to_str()
                    .expect("Found file with invalid UTF-8 file extension"),
                ".dcm",
                "{:?} does not have a .dcm file extension.",
                entry.path()
            );
            entry
        })
        .count()
        .await;
    assert_eq!(count, series.ndicom);
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
