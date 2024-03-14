use dicom::core::{DataDictionary, Tag};
use dicom::dictionary_std::StandardDataDictionary;

/// Error which might happen while receiving a DICOM series.
#[derive(thiserror::Error, Debug)]
pub(crate) enum AssociationError {
    #[error("Could not establish association.")]
    CouldNotEstablish(#[from] dicom::ul::association::server::Error),

    #[error("Failed to read incoming DICOM command")]
    FailedToReadCommand(dicom::object::ReadError),

    #[error("{0}")]
    CannotRespond(&'static str),

    #[error("Missing {}", name_of(.0))]
    MissingTag(Tag),

    #[error("Value for {} is not a number", name_of(.0))]
    InvalidNumber(Tag),

    #[error("Could not retrieve {}", name_of(.0))]
    CouldNotRetrieve(Tag),

    #[error("Missing presentation context")]
    MissingPresentationContext,

    #[error("Failed to read DICOM data object")]
    FailedToReadObject(#[from] dicom::object::ReadError),

    #[error("failed to build DICOM meta file information")]
    FailedToBuildMeta(dicom::object::meta::Error),
}

/// Get the standard name of a tag.
fn name_of(tag: &Tag) -> &'static str {
    StandardDataDictionary
        .by_tag(*tag)
        .map(|e| e.alias)
        .unwrap()
}
