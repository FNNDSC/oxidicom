//! Implementation of the **Light Oxidicom NotifiKations** encoding specification.
//!
//! Documentation: <https://github.com/FNNDSC/chrisproject.org/blob/d251b021be742bf9aab3596366d2a6b707faeba1/docs/oxidicom.md#light-oxidicom-notifikations-encoding>

use crate::error::DicomStorageError;
use crate::types::SeriesKey;
use bytes::Bytes;

pub const MESSAGE_NDICOM: u8 = 0x01;
pub const MESSAGE_ERROR: u8 = 0x02;
pub const DONE_MESSAGE: [u8; 1] = [0x00];

pub struct Lonk {
    pub series: SeriesKey,
    pub message: LonkMessage,
}

impl Lonk {
    pub fn done(series: SeriesKey) -> Self {
        Self {
            series,
            message: LonkMessage::Done,
        }
    }

    pub fn ndicom(series: SeriesKey, ndicom: u32) -> Self {
        Self {
            series,
            message: LonkMessage::Ndicom(ndicom),
        }
    }

    pub fn error(series: SeriesKey, error: DicomStorageError) -> Self {
        Self {
            series,
            message: LonkMessage::Error(error),
        }
    }
}

pub enum LonkMessage {
    Done,
    Ndicom(u32),
    Error(DicomStorageError),
}

impl LonkMessage {
    pub fn into_bytes(self) -> Bytes {
        match self {
            Self::Done => done_message(),
            Self::Ndicom(ndicom) => progress_message(ndicom),
            Self::Error(error) => error_message(error),
        }
    }
}

pub fn done_message() -> Bytes {
    Bytes::from_static(&DONE_MESSAGE)
}

/// Encode a LONK progress message.
pub fn progress_message(ndicom: u32) -> Bytes {
    let payload: Vec<u8> = [MESSAGE_NDICOM]
        .into_iter()
        .chain(ndicom.to_le_bytes())
        .collect();
    Bytes::from(payload)
}

/// Encode a LONK error message.
pub fn error_message(e: DicomStorageError) -> Bytes {
    let mut payload = e.to_string().into_bytes();
    payload.insert(0, MESSAGE_ERROR);
    Bytes::from(payload)
}

/// Get the NATS subject name for a series.
///
/// Specification: <https://github.com/FNNDSC/chrisproject.org/blob/d251b021be742bf9aab3596366d2a6b707faeba1/docs/oxidicom.md#oxidicom-nats-subjects>
pub fn subject_of(root_subject: impl std::fmt::Display, series: &SeriesKey) -> String {
    format!(
        "{}.{}.{}",
        root_subject,
        sanitize_subject_part(series.pacs_name.as_str()),
        sanitize_subject_part(&series.SeriesInstanceUID)
    )
}

/// Sanitize a string so that it only contains allowed characters for NATS subjects.
/// https://docs.nats.io/nats-concepts/subjects#characters-allowed-and-recommended-for-subject-names
fn sanitize_subject_part(name: &str) -> String {
    name.replace(&[' ', '.', '*', '>'], "_").replace('\0', "")
}
