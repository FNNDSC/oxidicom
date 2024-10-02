#![allow(non_snake_case)]

use crate::enums::SeriesEvent;
use crate::error::DicomStorageError;
use crate::registration_task::register_pacs_series;
use aliri_braid::braid;
use celery::task::Signature;
use time::macros::format_description;
use tokio::task::JoinHandle;

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

impl DicomInfo<SeriesPath> {
    /// Create task.
    pub fn into_task(self, ndicom: u32) -> Signature<register_pacs_series> {
        register_pacs_series::new(
            self.PatientID,
            self.StudyDate
                .format(format_description!("[year]-[month]-[day]"))
                .unwrap(),
            self.StudyInstanceUID,
            self.SeriesInstanceUID,
            self.pacs_name,
            self.path,
            ndicom,
            self.PatientName,
            self.PatientBirthDate,
            self.PatientAge,
            self.PatientSex,
            self.AccessionNumber,
            self.Modality,
            self.ProtocolName,
            self.StudyDescription,
            self.SeriesDescription,
        )
    }
}

/// An [SeriesEvent] for a pending task of writing a DICOM file to storage.
pub(crate) type PendingDicomInstance =
    SeriesEvent<JoinHandle<Result<(), DicomStorageError>>, DicomInfo<SeriesPath>>;

/// The set of metadata which uniquely identifies a DICOM series in *CUBE*.
///
/// https://github.com/FNNDSC/ChRIS_ultron_backEnd/blob/v6.1.0/chris_backend/pacsfiles/models.py#L60
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct SeriesKey {
    /// Series instance UID
    #[allow(non_snake_case)]
    pub SeriesInstanceUID: String,
    /// AE title of PACS the series was received from
    pub pacs_name: AETitle,
}

impl SeriesKey {
    pub fn new(series_instance_uid: String, pacs_name: AETitle) -> Self {
        Self {
            SeriesInstanceUID: series_instance_uid,
            pacs_name,
        }
    }
}
