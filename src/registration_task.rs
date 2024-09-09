//! Celery task definition of the PACSSeries registration function in CUBE,
//! for submitting tasks to CUBE (Python)'s celery worker from our Rust code.

#![allow(unused_variables)]
#![allow(unreachable_code)]
#![allow(non_snake_case)]
#![allow(clippy::too_many_arguments)]

use crate::types::SeriesPath;
use crate::AETitle;

/// A function stub with the same signature as the `register_pacs_series` celery task
/// in *CUBE*'s Python code.
///
/// ### `PatientAge` must be in days
///
/// `PatientAge` is in days and its type is `i32` because that is the column's type
///  in *CUBE*'s PostgreSQL database.
///
/// https://github.com/FNNDSC/pypx/blob/7b83154d7c6d631d81eac8c9c4a2fc164ccc2ebc/pypx/register.py#L459-L465
#[celery::task(name = "pacsfiles.tasks.register_pacs_series")]
pub fn register_pacs_series(
    PatientID: String,
    StudyDate: String,
    StudyInstanceUID: String,
    SeriesInstanceUID: String,
    pacs_name: AETitle,
    path: SeriesPath,
    ndicom: u32,
    PatientName: Option<String>,
    PatientBirthDate: Option<String>,
    PatientAge: Option<i32>,
    PatientSex: Option<String>,
    AccessionNumber: Option<String>,
    Modality: Option<String>,
    ProtocolName: Option<String>,
    StudyDescription: Option<String>,
    SeriesDescription: Option<String>,
) {
    unimplemented!()
}
