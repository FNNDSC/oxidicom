#![allow(non_snake_case)]

use std::borrow::Cow;
use std::fmt::Display;
use dicom::core::DataDictionary;

use dicom::dictionary_std::tags;
use dicom::object::{DefaultDicomObject, StandardDataDictionary, Tag};
use serde::{Deserialize, Serialize};

use crate::error::MissingRequiredTag;

#[derive(Serialize, Deserialize)]
pub struct PACSFile {
    pub path: String,
    pub PatientID: String,
    pub StudyDate: String,
    pub StudyInstanceUID: String,
    pub SeriesInstanceUID: String,
    pub pacs_name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub PatientName: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub PatientBirthDate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub PatientAge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub PatientSex: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub AccessionNumber: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub Modality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ProtocolName: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub StudyDescription: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub SeriesDescription: Option<String>,
}


impl PACSFile {
    pub fn new(pacs_name: String, dcm: &DefaultDicomObject) -> Result<Self, MissingRequiredTag> {
        let PatientID = ttr(dcm, tags::PATIENT_ID)?;
        let PatientName = tts(dcm, tags::PATIENT_NAME);
        let PatientBirthDate = tts(dcm, tags::PATIENT_BIRTH_DATE);
        let PatientAge = tts(dcm, tags::PATIENT_AGE);
        let StudyDescription = tts(dcm, tags::STUDY_DESCRIPTION);
        let AccessionNumber = tts(dcm, tags::ACCESSION_NUMBER);
        let StudyDate = ttr(dcm, tags::STUDY_DATE)?;
        let SeriesNumber = tt(dcm, tags::SERIES_NUMBER).map(MaybeU32::from);
        let SeriesDescription = tts(dcm, tags::SERIES_DESCRIPTION);
        let InstanceNumber = tt(dcm, tags::INSTANCE_NUMBER).map(MaybeU32::from);
        let SOPInstanceUID = tts(dcm, tags::SOP_INSTANCE_UID);
        let SeriesInstanceUID = ttr(dcm, tags::SERIES_INSTANCE_UID)?;

        // https://github.com/FNNDSC/pypx/blob/7b83154d7c6d631d81eac8c9c4a2fc164ccc2ebc/bin/px-push#L175-L195
        let path = format!(
            "{}-{}-{}-{}/{}-{}-{}/{:0>5}-{}-{}/{:0>4}-{}.dcm",
            // Patient
            PatientID.as_str(),
            PatientName.as_deref().unwrap_or(""),
            PatientBirthDate.as_deref().unwrap_or(""),
            PatientAge.as_deref().unwrap_or(""),
            // Study
            StudyDescription.as_deref().unwrap_or("StudyDescription"),
            AccessionNumber.as_deref().unwrap_or("AccessionNumber"),
            StudyDate.as_str(),
            // Series
            SeriesNumber.unwrap_or_else(|| MaybeU32::String("SeriesNumber".to_string())),
            SeriesDescription.as_deref().unwrap_or("SeriesDescription"),
            &hash(SeriesInstanceUID.as_str())[..7],
            // Instance
            InstanceNumber.unwrap_or_else(|| MaybeU32::String("InstanceNumber".to_string())),
            SOPInstanceUID.as_deref().unwrap_or("SOPInstanceUID")
        );

        let pacs_file = Self {
            path,
            pacs_name,
            PatientID,
            StudyDate,
            StudyInstanceUID: ttr(dcm, tags::STUDY_INSTANCE_UID)?,
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
        Ok(pacs_file)
    }
}

/// Required string tag
fn ttr(dcm: &DefaultDicomObject, tag: Tag) -> Result<String, MissingRequiredTag> {
    tt(dcm, tag).map(|s| s.to_string())
        .ok_or_else(|| MissingRequiredTag(name_of(tag).unwrap()))
}

/// Optional string tag
fn tts(dcm: &DefaultDicomObject, tag: Tag) -> Option<String> {
    tt(dcm, tag).map(|s| s.to_string())
}

/// Try to get the trimmed string value of a DICOM object.
fn tt(dcm: &DefaultDicomObject, tag: Tag) -> Option<&str> {
    dcm.element(tag)
        .ok()
        .and_then(|e| e.string().map(|s| s.trim()).ok())
}

/// Get the standard name of a tag.
fn name_of(tag: Tag) -> Option<&'static str> {
    // WHY SAG-anon has a DICOM tag (0019,0010)?
    StandardDataDictionary.by_tag(tag).map(|e| e.alias)
}

/// Something that is maybe a [u32], but in case it's not valid, is a [String].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
        write!(f, "{}", match self {
            MaybeU32::U32(i) => Cow::Owned(i.to_string()),
            MaybeU32::String(s) => Cow::Borrowed(s)
        })
    }
}

/// Produces the hash of the data as a hexidecimal string.
fn hash(data: &str) -> String {
    format!("{:x}", seahash::hash(data.as_bytes()))
}
