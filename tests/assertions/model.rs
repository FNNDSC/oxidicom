#![allow(non_snake_case)]

/// Parameters of the [oxidicom::register_pacs_series] celery task.
#[derive(Debug, Hash, Eq, PartialEq, serde::Deserialize)]
pub struct SeriesParams {
    pub PatientID: String,
    pub StudyDate: String,
    pub StudyInstanceUID: String,
    pub SeriesInstanceUID: String,
    pub pacs_name: String,
    pub path: String,
    pub ndicom: usize,
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
