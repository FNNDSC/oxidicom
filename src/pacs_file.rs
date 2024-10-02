use std::fmt::Display;

use crate::error::{name_of, DicomRequiredTagError, RequiredTagError};
use crate::patient_age::parse_age;
use crate::sanitize::sanitize_path;
use crate::types::{DicomFilePath, DicomInfo};
use crate::AETitle;
use dicom::dictionary_std::tags;
use dicom::object::{DefaultDicomObject, Tag};

/// A wrapper of [PacsFileRegistrationRequest] along with the [DefaultDicomObject] it was created from.
pub struct PacsFileRegistration {
    pub data: DicomInfo<DicomFilePath>,
    pub obj: DefaultDicomObject,
}

impl PacsFileRegistration {
    pub(crate) fn new(
        pacs_name: AETitle,
        obj: DefaultDicomObject,
    ) -> Result<(Self, Vec<BadTag>), DicomRequiredTagError> {
        match get_series_tags(pacs_name, &obj) {
            Ok((data, bad_tags)) => Ok((Self { data, obj }, bad_tags)),
            Err(error) => Err(DicomRequiredTagError { obj, error }),
        }
    }
}

#[allow(non_snake_case)]
fn get_series_tags(
    pacs_name: AETitle,
    dcm: &DefaultDicomObject,
) -> Result<(DicomInfo<DicomFilePath>, Vec<BadTag>), RequiredTagError> {
    let mut bad_tags = vec![];
    // required fields
    let StudyInstanceUID = ttr(dcm, tags::STUDY_INSTANCE_UID)?;
    let SeriesInstanceUID = ttr(dcm, tags::SERIES_INSTANCE_UID)?;
    let SOPInstanceUID = ttr(dcm, tags::SOP_INSTANCE_UID)?;
    let PatientID = ttr(dcm, tags::PATIENT_ID)?;
    let StudyDate_string = ttr(dcm, tags::STUDY_DATE)?;
    let StudyDate = parse_study_date(
        StudyDate_string.as_str(),
        &pacs_name,
        &SeriesInstanceUID,
    )?;

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
    let path = DicomFilePath::new(path);
    let pacs_file = DicomInfo {
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

/// An invalid DICOM tag key-value pair.
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

fn parse_study_date(
    s: &str,
    pacs_name: &AETitle,
    series_instance_uid: &str,
) -> Result<time::Date, RequiredTagError> {
    let da_format = time::macros::format_description!("[year][month][day]");
    time::Date::parse(s, &da_format)
        .or_else(|_| {
            let alt_format = time::macros::format_description!("[year]-[month]-[day]");
            let parsed = time::Date::parse(s, &alt_format);
            if parsed.is_ok() {
                tracing::warn!(
                    SeriesInstanceUID = series_instance_uid,
                    pacs_name = pacs_name.as_str(),
                    StudyDate = s,
                    "StudyDate is not a valid DICOM DA string, but was successfully parsed as YYYY-MM-DD"
                )
            }
            parsed
        })
        .map_err(|_| {
            RequiredTagError::Bad(BadTag {
                tag: tags::STUDY_DATE,
                value: Some(s.to_string()),
            })
        })
}

/// Required string tag
fn ttr(dcm: &DefaultDicomObject, tag: Tag) -> Result<String, RequiredTagError> {
    tts(dcm, tag).ok_or(RequiredTagError::Missing(tag))
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
