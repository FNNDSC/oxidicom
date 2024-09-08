//! Implementation of the **Light Oxidicom NotifiKations** encoding specification.
//!
//! Documentation: <https://github.com/FNNDSC/chrisproject.org/blob/d251b021be742bf9aab3596366d2a6b707faeba1/docs/oxidicom.md#light-oxidicom-notifikations-encoding>

use crate::error::DicomStorageError;
use crate::types::SeriesKey;
use bytes::Bytes;

const MESSAGE_NDICOM: u8 = 0x01;
const MESSAGE_ERROR: u8 = 0x02;
const DONE_MESSAGE: [u8; 1] = [0x00];

pub(crate) fn done_message() -> Bytes {
    Bytes::from_static(&DONE_MESSAGE)
}

/// Encode a LONK progress message.
pub(crate) fn progress_message(ndicom: u32) -> Bytes {
    let payload: Vec<u8> = [MESSAGE_NDICOM]
        .into_iter()
        .chain(ndicom.to_le_bytes())
        .collect();
    Bytes::from(payload)
}

/// Encode a LONK error message.
pub(crate) fn error_message(e: DicomStorageError) -> Bytes {
    let mut payload = e.to_string().into_bytes();
    payload.insert(0, MESSAGE_ERROR);
    Bytes::from(payload)
}

/// Get the NATS subject name for a series.
///
/// Specification: <https://github.com/FNNDSC/chrisproject.org/blob/d251b021be742bf9aab3596366d2a6b707faeba1/docs/oxidicom.md#oxidicom-nats-subjects>
pub(crate) fn subject_of(series: &SeriesKey) -> String {
    format!(
        "oxidicom.{}.{}",
        &series.pacs_name,
        sanitize_subject_part(&series.SeriesInstanceUID)
    )
}

/// Sanitize a string so that it only contains allowed characters for NATS subjects.
/// https://docs.nats.io/nats-concepts/subjects#characters-allowed-and-recommended-for-subject-names
fn sanitize_subject_part(name: &str) -> String {
    name.replace(&[' ', '.', '*', '>'], "_").replace('\0', "")
}
