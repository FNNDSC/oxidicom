#![allow(non_snake_case)]

use crate::dicomrs_settings::ClientAETitle;
use crate::pacs_file::PacsFileRegistrationRequest;
use ulid::Ulid;

/// The set of fields of a [PacsFileRegistrationRequest] which uniquely identifies a DICOM series
/// in CUBE.
///
/// For well-behaved PACS, `SeriesInstanceUID` would be all you need. However, we do not assume
/// the PACS is well-behaved nor the DICOM tags to be 100% valid.
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct SeriesKeySet {
    pub dir_path: String,
    pub PatientID: String,
    pub StudyDate: time::Date,
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

/// The special `pacs_name` used by Oxidicom to register "Oxidicom Custom Metadata" files to CUBE.
///
/// Note: must be 20 characters or fewer. This is a restriction of CUBE.
pub const OXIDICOM_CUSTOM_PACS_NAME: &str = "org.fnndsc.oxidicom";

impl SeriesKeySet {
    /// Serialize a key-value pair from an association as an "Oxidicom Custom Metadata" file.
    pub(crate) fn to_oxidicom_custom_pacsfile(
        self,
        association_ulid: Ulid,
        key: &str,
        value: impl AsRef<str>,
    ) -> PacsFileRegistrationRequest {
        let path = format!(
            "SERVICES/PACS/{}/{}/{}/{}={}",
            OXIDICOM_CUSTOM_PACS_NAME,
            self.dir_path,
            association_ulid,
            key,
            value.as_ref()
        );
        PacsFileRegistrationRequest {
            path,
            PatientID: self.PatientID,
            StudyDate: self.StudyDate,
            StudyInstanceUID: self.StudyInstanceUID,
            SeriesInstanceUID: self.SeriesInstanceUID,
            pacs_name: ClientAETitle::from_static(OXIDICOM_CUSTOM_PACS_NAME),
            PatientName: None,
            PatientBirthDate: None,
            PatientAge: None,
            PatientSex: None,
            AccessionNumber: None,
            Modality: None,
            ProtocolName: Some(key.to_string()),
            StudyDescription: None,
            SeriesDescription: Some(value.as_ref().to_string()),
        }
    }
}
