#![allow(non_snake_case)]

use crate::dicomrs_options::ClientAETitle;
use crate::pacs_file::PacsFileRegistrationRequest;

/// The set of fields of a [PacsFileRegistrationRequest] which uniquely identifies a DICOM series
/// in CUBE.
///
/// For well-behaved PACS, `SeriesInstanceUID` would be all you need. However, we do not assume
/// the PACS is well-behaved nor the DICOM tags to be 100% valid.
#[derive(Hash, PartialEq, Eq)]
pub struct SeriesKeySet {
    pub dir_path: String,
    pub PatientID: String,
    pub StudyDate: String,
    pub StudyInstanceUID: String,
    pub SeriesInstanceUID: String,
    pub pacs_name: ClientAETitle,
}

impl From<PacsFileRegistrationRequest> for SeriesKeySet {
    fn from(
        PacsFileRegistrationRequest {
            path,
            PatientID,
            StudyDate,
            StudyInstanceUID,
            SeriesInstanceUID,
            pacs_name,
            ..
        }: PacsFileRegistrationRequest,
    ) -> Self {
        Self {
            dir_path: dirname(&path).to_string(),
            PatientID,
            StudyDate,
            StudyInstanceUID,
            SeriesInstanceUID,
            pacs_name,
        }
    }
}

fn dirname(s: &str) -> &str {
    s.rsplit_once('/')
        .map(|(l, _)| l)
        .expect("fname does not contain a slash.")
}
