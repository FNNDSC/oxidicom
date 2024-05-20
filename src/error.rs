use crate::pacs_file::BadTag;
use dicom::core::DataDictionary;
use dicom::dictionary_std::StandardDataDictionary;
use dicom::object::Tag;
use reqwest::blocking::Response;
use reqwest::StatusCode;

#[derive(thiserror::Error, Debug)]
pub enum ChrisPacsError {
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    Write(#[from] dicom::object::WriteError),

    #[error(transparent)]
    MissingTag(#[from] RequiredTagError),

    #[error(transparent)]
    Cube(#[from] CubeError),

    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

#[derive(thiserror::Error, Debug)]
#[error("({status:?} {reason:?}): {text:?}")]
pub struct CubeError {
    pub status: StatusCode,
    pub reason: &'static str,
    pub text: Result<String, reqwest::Error>,
    pub source: reqwest::Error,
}

#[derive(thiserror::Error, Debug)]
pub enum RequiredTagError {
    #[error("DICOM file does not have the required tag: {}", name_of(.0))]
    Missing(Tag),
    #[error("Illegal value for tag {}={:?}", name_of(&.0.tag), .0.value)]
    Bad(BadTag),
}

/// Get the standard name of a tag.
pub(crate) fn name_of(tag: &Tag) -> &'static str {
    StandardDataDictionary
        .by_tag(*tag)
        .map(|e| e.alias)
        .unwrap_or("UNKNOWN TAG")
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum RequestError {
    #[error(transparent)]
    Cube(#[from] CubeError),

    #[error(transparent)]
    Base(#[from] reqwest::Error),
}

impl RequestError {
    pub fn status(&self) -> Option<StatusCode> {
        match self {
            Self::Cube(e) => Some(e.status),
            Self::Base(e) => e.status(),
        }
    }
}

impl From<RequestError> for ChrisPacsError {
    fn from(value: RequestError) -> Self {
        match value {
            RequestError::Cube(e) => Self::Cube(e),
            RequestError::Base(e) => Self::Request(e),
        }
    }
}

pub(crate) fn check(res: Response) -> Result<Response, RequestError> {
    match res.error_for_status_ref() {
        Ok(_) => Ok(res),
        Err(source) => {
            let error = CubeError {
                status: res.status(),
                reason: res.status().canonical_reason().unwrap_or("unknown reason"),
                text: res.text(),
                source,
            };
            Err(RequestError::Cube(error))
        }
    }
}
