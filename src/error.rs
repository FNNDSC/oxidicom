use dicom::core::DataDictionary;
use dicom::dictionary_std::StandardDataDictionary;
use dicom::object::Tag;
use reqwest::blocking::Response;

#[derive(thiserror::Error, Debug)]
pub enum ChrisPacsError {
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    Write(#[from] dicom::object::WriteError),

    #[error(transparent)]
    MissingTag(#[from] MissingRequiredTag),

    #[error("({status:?} {reason:?}): {text:?}")]
    Cube {
        status: reqwest::StatusCode,
        reason: &'static str,
        text: Result<String, reqwest::Error>,
        source: reqwest::Error,
    },

    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

#[derive(thiserror::Error, Debug)]
#[error("DICOM file does not have the required tag: {}", name_of(.0))]
pub struct MissingRequiredTag(pub Tag);

/// Get the standard name of a tag.
pub(crate) fn name_of(tag: &Tag) -> &'static str {
    StandardDataDictionary
        .by_tag(*tag)
        .map(|e| e.alias)
        .unwrap_or("UNKNOWN TAG")
}

pub(crate) fn check(res: Response) -> Result<Response, ChrisPacsError> {
    match res.error_for_status_ref() {
        Ok(_) => Ok(res),
        Err(source) => {
            let status = res.status();
            let reason = status.canonical_reason().unwrap_or("unknown reason");
            let text = res.text();
            Err(ChrisPacsError::Cube {
                status,
                reason,
                text,
                source,
            })
        }
    }
}
