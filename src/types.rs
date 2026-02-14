#![allow(non_snake_case)]

use crate::enums::SeriesEvent;
use crate::error::DicomStorageError;
use aliri_braid::braid;
use serde_json::Value;
use time::macros::format_description;
use tokio::task::JoinHandle;
use ulid::Ulid;

pub(crate) type CubeRegistrationParams = (DicomInfo<SeriesPath>, u32);

/// Path in storage to a DICOM instance file.
#[braid(serde)]
pub(crate) struct DicomFilePath;

/// Path in storage to a DICOM series folder.
#[braid(serde)]
pub(crate) struct SeriesPath;

/// The AE title of a peer PACS server pushing DICOMs to us.
#[braid(serde)]
pub struct AETitle;

impl From<DicomFilePath> for SeriesPath {
    fn from(path: DicomFilePath) -> Self {
        path.as_str()
            .rsplit_once('/')
            .map(|(dir, _fname)| dir)
            .map(Self::from)
            .unwrap()
    }
}

impl From<DicomInfo<DicomFilePath>> for DicomInfo<SeriesPath> {
    fn from(value: DicomInfo<DicomFilePath>) -> Self {
        Self {
            PatientID: value.PatientID,
            StudyDate: value.StudyDate,
            StudyInstanceUID: value.StudyInstanceUID,
            SeriesInstanceUID: value.SeriesInstanceUID,
            pacs_name: value.pacs_name,
            path: value.path.into(),
            PatientName: value.PatientName,
            PatientBirthDate: value.PatientBirthDate,
            PatientAge: value.PatientAge,
            PatientSex: value.PatientSex,
            AccessionNumber: value.AccessionNumber,
            Modality: value.Modality,
            ProtocolName: value.ProtocolName,
            StudyDescription: value.StudyDescription,
            SeriesDescription: value.SeriesDescription,
        }
    }
}

/// The DICOM series metadata needed for *CUBE*'s serializer to register a PACS series
/// as a `PACSSeries` object.
#[derive(Debug, Clone)]
pub(crate) struct DicomInfo<P> {
    pub PatientID: String,
    pub StudyDate: time::Date,
    pub StudyInstanceUID: String,
    pub SeriesInstanceUID: String,
    pub pacs_name: AETitle,
    pub path: P,
    pub PatientName: Option<String>,
    pub PatientBirthDate: Option<String>,
    pub PatientAge: Option<i32>,
    pub PatientSex: Option<String>,
    pub AccessionNumber: Option<String>,
    pub Modality: Option<String>,
    pub ProtocolName: Option<String>,
    pub StudyDescription: Option<String>,
    pub SeriesDescription: Option<String>,
}

#[derive(serde::Serialize)]
pub(crate) struct LoginParams {
    pub username: String,
    pub password: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CollectionJSON {
    pub collection: CollectionJSONCollection,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CollectionJSONCollection {
    pub version: String,
    pub href: String,
    pub items: Vec<CollectionJSONItem>,
    pub links: Vec<CollectionJSONLink>,
    pub total: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CollectionJSONItem {
    pub data: Vec<CollectionJSONData>,
    pub href: String,
    pub links: Vec<CollectionJSONLink>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CollectionJSONLink {
    pub rel: String,
    pub href: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct CollectionJSONData {
    pub name: String,
    pub value: Value,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct AuthToken {
    pub token: String,
}

#[derive(serde::Serialize)]
pub(crate) struct DicomInfoWithNDicom {
    pub PatientID: String,
    pub StudyDate: String,
    pub StudyInstanceUID: String,
    pub SeriesInstanceUID: String,
    pub pacs_name: AETitle,
    pub path: SeriesPath,
    pub PatientName: Option<String>,
    pub PatientBirthDate: Option<String>,
    pub PatientAge: Option<i32>,
    pub PatientSex: Option<String>,
    pub AccessionNumber: Option<String>,
    pub Modality: Option<String>,
    pub ProtocolName: Option<String>,
    pub StudyDescription: Option<String>,
    pub SeriesDescription: Option<String>,
    pub ndicom: u32,
}

impl DicomInfo<SeriesPath> {
    pub fn into_dicominfo_with_ndicom(self, ndicom: u32) -> DicomInfoWithNDicom {
        let accessionNumber = match self.AccessionNumber {
            Some(b) => b,
            None => "".to_string(),
        };
        DicomInfoWithNDicom {
            PatientID: self.PatientID,
            StudyDate: self
                .StudyDate
                .format(format_description!("[year]-[month]-[day]"))
                .unwrap(),
            StudyInstanceUID: self.StudyInstanceUID,
            SeriesInstanceUID: self.SeriesInstanceUID,
            pacs_name: self.pacs_name,
            path: self.path,
            PatientName: self.PatientName,
            PatientBirthDate: self.PatientBirthDate,
            PatientAge: self.PatientAge,
            PatientSex: self.PatientSex,
            AccessionNumber: Some(accessionNumber),
            Modality: self.Modality,
            ProtocolName: self.ProtocolName,
            StudyDescription: self.StudyDescription,
            SeriesDescription: self.SeriesDescription,
            ndicom,
        }
    }
}

/// An [SeriesEvent] for a pending task of writing a DICOM file to storage.
pub(crate) type PendingDicomInstance =
    SeriesEvent<JoinHandle<Result<(), DicomStorageError>>, DicomInfo<SeriesPath>>;

/// The set of metadata which uniquely identifies a DICOM series in *CUBE* per DICOM association.
///
/// https://github.com/FNNDSC/ChRIS_ultron_backEnd/blob/v6.1.0/chris_backend/pacsfiles/models.py#L60
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SeriesKey {
    /// Series instance UID
    #[allow(non_snake_case)]
    pub SeriesInstanceUID: String,
    /// AE title of PACS the series was received from
    pub pacs_name: AETitle,
    /// The DICOM association ULID.
    pub association: Ulid,
}

impl SeriesKey {
    pub fn new(series_instance_uid: String, pacs_name: AETitle, association: Ulid) -> Self {
        Self {
            SeriesInstanceUID: series_instance_uid,
            pacs_name,
            association,
        }
    }
}
