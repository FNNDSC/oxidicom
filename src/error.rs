use crate::pacs_file::BadTag;
use dicom::core::DataDictionary;
use dicom::dictionary_std::StandardDataDictionary;
use dicom::object::{DefaultDicomObject, Tag};

/// Error storing a DICOM object as a file to _CUBE_ storage.
#[derive(thiserror::Error, Debug)]
pub enum DicomStorageError {
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    Write(#[from] dicom::object::WriteError),

    #[error(transparent)]
    MissingTag(#[from] RequiredTagError),
}

impl From<DicomRequiredTagError> for DicomStorageError {
    fn from(value: DicomRequiredTagError) -> Self {
        DicomStorageError::MissingTag(value.error)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RequiredTagError {
    #[error("DICOM file does not have the required tag: {}", name_of(.0))]
    Missing(Tag),
    #[error("Illegal value for tag {}={:?}", name_of(&.0.tag), .0.value)]
    Bad(BadTag),
}

#[derive(thiserror::Error, Debug)]
#[error("{error}")]
pub struct DicomRequiredTagError {
    pub obj: DefaultDicomObject,
    pub error: RequiredTagError,
}

/// Get the standard name of a tag.
pub(crate) fn name_of(tag: &Tag) -> &'static str {
    StandardDataDictionary
        .by_tag(*tag)
        .map(|e| e.alias)
        .unwrap_or("UNKNOWN TAG")
}
