use std::borrow::Cow;

use opentelemetry::trace::{Status, TraceContextExt, Tracer};
use opentelemetry::{global, KeyValue};

use crate::cube_client::CubePacsStorageClient;
use crate::error::ChrisPacsError;
use crate::findscu::FindScuParameters;
use crate::pacs_file::{PacsFileRegistrationRequest, PacsFileResponse};
use crate::series_key_set::SeriesKeySet;

/// Send a C-FIND request to the PACS server to obtain `NumberOfSeriesRelatedInstances`.
/// Store this information in _CUBE_ via the "Oxidicom Custom Metadata" spec.
pub(crate) fn getset_number_of_series_related_instances(
    client: &CubePacsStorageClient,
    uuid: Uuid,
    params: Option<FindScuParameters>,
    series: SeriesKeySet,
) -> Result<(), ChrisPacsError> {
    let tracer = global::tracer(env!("CARGO_PKG_NAME"));
    tracer.in_span("getset_number_of_series_related_instances", |cx| {
        let number_of_series_related_instances = params
            .and_then(|p| p.get_number_of_series_related_instances())
            .map(|num| num.to_string())
            .unwrap_or("unknown".to_string());
        let pacs_file = series.to_oxidicom_custom_pacsfile(
            uuid,
            "NumberOfSeriesRelatedInstances",
            number_of_series_related_instances,
        );
        match store_blank_wrapper(client, &pacs_file) {
            Ok(_) => {
                cx.span().set_status(Status::Ok);
                Ok(())
            }
            Err(e) => {
                cx.span().set_status(Status::Error {
                    description: Cow::Borrowed("Failed to register blank file to CUBE."),
                });
                Err(e)
            }
        }
    })
}

/// For every series, push the "Oxidicom Custom Metadata" `OxidicomAttemptedPushCount=*` file to CUBE.
///
/// This function includes OpenTelemetry spans.
pub(crate) fn register_all_attempted_pushes(
    client: &CubePacsStorageClient,
    uuid: Uuid,
    series: impl Iterator<Item = (SeriesKeySet, usize)>,
) -> Vec<Result<PacsFileResponse, ChrisPacsError>> {
    let tracer = global::tracer(env!("CARGO_PKG_NAME"));
    tracer.in_span("register_all_attempted_pushes", |cx| {
        cx.span().set_attribute(KeyValue::new(
            "association_ulid",
            uuid.hyphenated().to_string(),
        ));
        let results: Vec<_> = series
            .map(|(series, count)| register_attempted_series_push(client, uuid, series, count))
            .collect();
        if results.iter().all(|r| r.is_ok()) {
            cx.span().set_status(Status::Ok)
        } else {
            cx.span().set_status(Status::Error {
                description: Cow::Borrowed("One or more of the requests had an error."),
            })
        };
        results
    })
}

/// Register an "Oxidicom Custom Metadata" `OxidicomAttemptedPushCount=*` file for a single series
/// to CUBE.
fn register_attempted_series_push(
    client: &CubePacsStorageClient,
    uuid: Uuid,
    series: SeriesKeySet,
    count: usize,
) -> Result<PacsFileResponse, ChrisPacsError> {
    let pacs_file =
        series.to_oxidicom_custom_pacsfile(uuid, "OxidicomAttemptedPushCount", count.to_string());
    store_blank_wrapper(client, &pacs_file)
}

fn store_blank_wrapper(
    client: &CubePacsStorageClient,
    pacs_file: &PacsFileRegistrationRequest,
) -> Result<PacsFileResponse, ChrisPacsError> {
    let tracer = global::tracer(env!("CARGO_PKG_NAME"));
    tracer.in_span("register_oxidicom_custom_metadata", |cx| {
        match client.store_blank(&pacs_file) {
            Ok(res) => {
                // The "Oxidicom Custom Metadata" spec requires that
                // the key is stored as ProtocolName and the value is stored as SeriesDescription
                let key = res.ProtocolName.clone().unwrap_or_else(|| "".to_string());
                let value = res
                    .SeriesDescription
                    .clone()
                    .unwrap_or_else(|| "".to_string());
                cx.span().set_attributes([
                    KeyValue::new("StudyInstanceUID", res.StudyInstanceUID.to_string()),
                    KeyValue::new("SeriesInstanceUID", res.SeriesInstanceUID.to_string()),
                    KeyValue::new("fname", res.fname.to_string()),
                    KeyValue::new("url", res.url.to_string()),
                    KeyValue::new("oxidicom.custom.key", key),
                    KeyValue::new("oxidicom.custom.value", value),
                ]);
                cx.span().set_status(Status::Ok);
                Ok(res)
            }
            Err(e) => {
                cx.span().set_status(Status::Error {
                    description: Cow::Owned(e.to_string()),
                });
                Err(e)
            }
        }
    })
}
