#[derive(thiserror::Error, Debug)]
pub enum ChrisPacsError {
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),

    #[error(transparent)]
    Write(#[from] dicom::object::WriteError),
}
