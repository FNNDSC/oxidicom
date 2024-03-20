use crate::cube_client::CubePacsStorageClient;
use crate::error::ChrisPacsError;
use crate::findscu::FindScuParameters;
use crate::pacs_file::{PacsFileRegistrationRequest, PacsFileResponse};
use crate::series_key_set::SeriesKeySet;
use opentelemetry::trace::{Status, TraceContextExt, Tracer};
use opentelemetry::{global, KeyValue};
use std::borrow::Cow;
use uuid::Uuid;

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

fn store_blank_wrapper(
    client: &CubePacsStorageClient,
    pacs_file: &PacsFileRegistrationRequest,
) -> Result<PacsFileResponse, ChrisPacsError> {
    let tracer = global::tracer(env!("CARGO_PKG_NAME"));
    tracer.in_span("store_blank", |cx| match client.store_blank(&pacs_file) {
        Ok(res) => {
            cx.span().set_attributes([
                KeyValue::new("StudyInstanceUID", res.StudyInstanceUID.to_string()),
                KeyValue::new("SeriesInstanceUID", res.SeriesInstanceUID.to_string()),
                KeyValue::new("fname", res.fname.to_string()),
                KeyValue::new("url", res.url.to_string()),
            ]);
            Ok(res)
        }
        Err(e) => {
            cx.span().set_status(Status::Error {
                description: Cow::Owned(e.to_string()),
            });
            Err(e)
        }
    })
}
