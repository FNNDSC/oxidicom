#[derive(thiserror::Error, Debug)]
pub enum ChrisPacsError {
    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),

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
    #[error(transparent)]
    MiddlewareRequest(#[from] reqwest_middleware::Error),
}

#[derive(thiserror::Error, Debug)]
#[error("DICOM file does not have the required tag: \"{0}\"")]
pub struct MissingRequiredTag(pub &'static str);

pub(crate) async fn check(res: reqwest::Response) -> Result<reqwest::Response, ChrisPacsError> {
    match res.error_for_status_ref() {
        Ok(_) => Ok(res),
        Err(source) => {
            let status = res.status();
            let reason = status.canonical_reason().unwrap_or("unknown reason");
            let text = res.text().await;
            Err(ChrisPacsError::Cube {
                status,
                reason,
                text,
                source,
            })
        }
    }
}
