#[derive(thiserror::Error, Debug)]
pub enum ChrisPacsError {
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),

    #[error(transparent)]
    Write(#[from] dicom::object::WriteError),

    #[error(transparent)]
    MissingTag(#[from] MissingRequiredTag)
}

#[derive(thiserror::Error, Debug)]
#[error("DICOM file does not have the required tag: \"{0}\"")]
pub struct MissingRequiredTag(pub &'static str);
