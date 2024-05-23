//! Request and response bodies for the CUBE `api/v1/pacsfiles/` API endpoint.
//!
//! ## Notes
//!
//! CUBE will normalize the DICOM DA format "YYYYMMDD" to "YYYY-MM-DD"
//! (so it's not something we need to worry about).
//!
//! `PatientAge` should be in days.
//! https://github.com/FNNDSC/pypx/blob/7b83154d7c6d631d81eac8c9c4a2fc164ccc2ebc/pypx/register.py#L459-L465
#![allow(non_snake_case)]

use std::fmt::Display;

use crate::dicomrs_options::ClientAETitle;
use dicom::dictionary_std::tags;
use dicom::object::{DefaultDicomObject, Tag};

use crate::error::{name_of, RequiredTagError};
use crate::patient_age::parse_age;
use crate::sanitize::sanitize_path;

/// Data necessary to register a DICOM file to CUBE's database in the `pacsfiles_pacsfile` table.
///
/// Historically, this struct represented the JSON payload to `POST api/v1/pacs/`. However,
/// we register files directly to the database instead of via CUBE for performance reasons.
#[derive(Debug, Clone)]
pub struct PacsFileRegistrationRequest {
    pub path: String,
    pub PatientID: String,
    pub StudyDate: time::Date,
    pub StudyInstanceUID: String,
    pub SeriesInstanceUID: String,
    pub pacs_name: ClientAETitle,

    pub PatientName: Option<String>,
    pub PatientBirthDate: Option<String>,
    pub PatientAge: Option<i32>, // i32 because PostgreSQL
    pub PatientSex: Option<String>,
    pub AccessionNumber: Option<String>,
    pub Modality: Option<String>,
    pub ProtocolName: Option<String>,
    pub StudyDescription: Option<String>,
    pub SeriesDescription: Option<String>,
}

#[derive(Debug)]
pub struct BadTag {
    pub tag: Tag,
    pub value: Option<String>,
}

impl Display for BadTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}={:?}", name_of(&self.tag), self.value)
    }
}

impl PacsFileRegistrationRequest {
    pub fn new(
        pacs_name: ClientAETitle,
        dcm: &DefaultDicomObject,
    ) -> Result<(Self, Vec<BadTag>), RequiredTagError> {
        let mut bad_tags = vec![];
        // required fields
        let StudyInstanceUID = ttr(dcm, tags::STUDY_INSTANCE_UID)?;
        let SeriesInstanceUID = ttr(dcm, tags::SERIES_INSTANCE_UID)?;
        let SOPInstanceUID = ttr(dcm, tags::SOP_INSTANCE_UID)?;
        let PatientID = ttr(dcm, tags::PATIENT_ID)?;
        let StudyDate_string = ttr(dcm, tags::STUDY_DATE)?; // required by CUBE
        let StudyDate_format = time::macros::format_description!("[year][month][day]"); // DICOM DA format
        let StudyDate = time::Date::parse(&StudyDate_string, &StudyDate_format).map_err(|_| {
            RequiredTagError::Bad(BadTag {
                tag: tags::STUDY_DATE,
                value: Some(StudyDate_string.to_string()),
            })
        })?;

        // optional values
        let PatientName = tts(dcm, tags::PATIENT_NAME);
        let PatientBirthDate = tts(dcm, tags::PATIENT_BIRTH_DATE);
        let StudyDescription = tts(dcm, tags::STUDY_DESCRIPTION);
        let AccessionNumber = tts(dcm, tags::ACCESSION_NUMBER);
        let SeriesDescription = tts(dcm, tags::SERIES_DESCRIPTION);

        // SeriesNumber and InstanceNumber are not fields of a ChRIS PACSFile.
        // They should be integers, and they also should appear in the fname.
        let InstanceNumber = tt(dcm, tags::INSTANCE_NUMBER).map(MaybeU32::from);
        let SeriesNumber = tt(dcm, tags::SERIES_NUMBER).map(MaybeU32::from);

        // Numerical value
        let PatientAgeStr = tt(dcm, tags::PATIENT_AGE);
        let PatientAge = PatientAgeStr.and_then(|age| {
            let num = parse_age(age.trim());
            if num.is_none() {
                bad_tags.push(BadTag {
                    tag: tags::PATIENT_AGE,
                    value: Some(age.to_string()),
                })
            };
            num
        });

        // https://github.com/FNNDSC/pypx/blob/7b83154d7c6d631d81eac8c9c4a2fc164ccc2ebc/bin/px-push#L175-L195
        let path = format!(
            "SERVICES/PACS/{}/{}-{}-{}/{}-{}-{}/{:0>5}-{}-{}/{:0>4}-{}.dcm",
            sanitize_path(&pacs_name),
            // Patient
            sanitize_path(PatientID.as_str()),
            sanitize_path(PatientName.as_deref().unwrap_or("")),
            sanitize_path(PatientBirthDate.as_deref().unwrap_or("")),
            // Study
            sanitize_path(StudyDescription.as_deref().unwrap_or("StudyDescription")),
            sanitize_path(AccessionNumber.as_deref().unwrap_or("AccessionNumber")),
            sanitize_path(StudyDate_string.as_str()),
            // Series
            SeriesNumber.unwrap_or_else(|| MaybeU32::String("SeriesNumber".to_string())),
            sanitize_path(SeriesDescription.as_deref().unwrap_or("SeriesDescription")),
            &hash(SeriesInstanceUID.as_str())[..7],
            // Instance
            InstanceNumber.unwrap_or_else(|| MaybeU32::String("InstanceNumber".to_string())),
            sanitize_path(SOPInstanceUID)
        );

        let pacs_file = Self {
            path,
            pacs_name,
            PatientID,
            StudyDate,
            StudyInstanceUID,
            SeriesInstanceUID,
            PatientName,
            PatientBirthDate,
            PatientAge,
            PatientSex: tts(dcm, tags::PATIENT_SEX),
            AccessionNumber,
            Modality: tts(dcm, tags::MODALITY),
            ProtocolName: tts(dcm, tags::PROTOCOL_NAME),
            StudyDescription,
            SeriesDescription,
        };
        Ok((pacs_file, bad_tags))
    }
}

/// Required string tag
fn ttr(dcm: &DefaultDicomObject, tag: Tag) -> Result<String, RequiredTagError> {
    tts(dcm, tag).ok_or_else(|| RequiredTagError::Missing(tag))
}

/// Optional string tag (with null bytes removed)
fn tts(dcm: &DefaultDicomObject, tag: Tag) -> Option<String> {
    tt(dcm, tag).map(|s| s.replace('\0', ""))
}

/// Try to get the trimmed string value of a DICOM object.
/// (This function is marginally more efficient than [tts].)
pub(crate) fn tt(dcm: &DefaultDicomObject, tag: Tag) -> Option<&str> {
    dcm.element(tag)
        .ok()
        .and_then(|e| e.string().map(|s| s.trim()).ok())
}

/// Something that is maybe a [u32], but in case it's not valid, is a [String].
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum MaybeU32 {
    U32(u32),
    String(String),
}

impl From<&str> for MaybeU32 {
    fn from(value: &str) -> Self {
        value
            .parse()
            .map(Self::U32)
            .unwrap_or_else(|_| MaybeU32::String(value.to_string()))
    }
}

impl Display for MaybeU32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaybeU32::U32(i) => i.fmt(f),
            MaybeU32::String(s) => s.fmt(f),
        }
    }
}

/// Produces the hash of the data as a hexidecimal string.
fn hash(data: &str) -> String {
    format!("{:x}", seahash::hash(data.as_bytes()))
}

#[derive(Debug, serde::Deserialize)]
pub struct PacsFileResponse {
    pub url: String,
    pub id: u32,
    pub creation_date: String,
    pub fname: String,
    pub fsize: u32,

    pub PatientID: String,
    pub PatientBirthDate: Option<String>,
    pub PatientAge: Option<u32>,
    pub PatientSex: Option<String>,
    pub StudyDate: String,

    pub AccessionNumber: Option<String>,
    pub Modality: Option<String>,
    pub ProtocolName: Option<String>,
    pub StudyInstanceUID: String,
    pub SeriesInstanceUID: String,
    pub SeriesDescription: Option<String>,
    pub pacs_identifier: String,
    pub file_resource: String,
}
