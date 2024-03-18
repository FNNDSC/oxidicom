use crate::dicomrs_options::ClientAETitle;
use camino::Utf8PathBuf;
use dicom::object::DefaultDicomObject;
use reqwest::StatusCode;
use std::time::Duration;

use crate::error::{check, ChrisPacsError, MissingRequiredTag, RequestError};
use crate::pacs_file::{BadTag, PacsFileRegistrationRequest, PacsFileResponse};

pub struct CubePacsStorageClient {
    client: reqwest::blocking::Client,
    retries: u16,
    url: String,
    username: String,
    password: String,
    dir: Utf8PathBuf,
}

impl CubePacsStorageClient {
    pub(crate) fn new(
        url: String,
        username: String,
        password: String,
        dir: Utf8PathBuf,
        retries: u16,
    ) -> Self {
        Self {
            url,
            client: reqwest::blocking::ClientBuilder::new()
                .use_rustls_tls()
                .build()
                .unwrap(),
            username,
            password,
            dir,
            retries,
        }
    }

    pub(crate) fn store(
        &self,
        pacs_file: &PacsFileRegistration,
    ) -> Result<PacsFileResponse, ChrisPacsError> {
        let dst = self.dir.join(&pacs_file.request.path);
        if let Some(parent) = dst.parent() {
            fs_err::create_dir_all(parent)?;
        }
        pacs_file.obj.write_to_file(dst)?;
        self.register_file(&pacs_file.request)
    }

    fn register_file(
        &self,
        file: &PacsFileRegistrationRequest,
    ) -> Result<PacsFileResponse, ChrisPacsError> {
        let mut last_error = None;
        let max_retries = self.retries + 1;
        for attempt in 1..max_retries {
            match self.send_register_request(file) {
                Ok(data) => return Ok(data),
                Err(e) => {
                    if should_retry(&e) {
                        if attempt != self.retries {
                            let duration = backoff(attempt);
                            tracing::warn!(
                                "Error from CUBE: {:?}. Going to retry after {}s",
                                &e,
                                duration.as_secs()
                            );
                            std::thread::sleep(duration);
                        }
                        last_error = Some(e);
                    } else {
                        return Err(e.into());
                    }
                }
            }
        }
        Err(last_error.unwrap().into())
    }

    fn send_register_request(
        &self,
        file: &PacsFileRegistrationRequest,
    ) -> Result<PacsFileResponse, RequestError> {
        let res = self
            .client
            .post(&self.url)
            .basic_auth(&self.username, Some(&self.password))
            .header(reqwest::header::ACCEPT, "application/json")
            .json(file)
            .send()?;
        let data = check(res)?.json()?;
        return Ok(data);
    }
}

/// A wrapper of [PacsFileRegistrationRequest] along with the [DefaultDicomObject] it was created from.
pub(crate) struct PacsFileRegistration {
    pub(crate) request: PacsFileRegistrationRequest,
    pub(crate) obj: DefaultDicomObject,
}

impl PacsFileRegistration {
    /// Wraps [PacsFileRegistrationRequest::new], returning the same result but with ownership of
    /// the given [DefaultDicomObject].
    pub(crate) fn new(
        pacs_name: ClientAETitle,
        obj: DefaultDicomObject,
    ) -> Result<(Self, Vec<BadTag>), (MissingRequiredTag, DefaultDicomObject)> {
        match PacsFileRegistrationRequest::new(pacs_name, &obj) {
            Ok((request, bad_tags)) => Ok((Self { request, obj }, bad_tags)),
            Err(e) => Err((e, obj)),
        }
    }
}

fn should_retry(e: &RequestError) -> bool {
    e.status()
        .as_ref()
        .map(|status| RETRYABLE_STATUS.iter().find(|s| s == &status).is_some())
        .unwrap_or(false)
}

const RETRYABLE_STATUS: [StatusCode; 8] = [
    StatusCode::INTERNAL_SERVER_ERROR,
    StatusCode::BAD_GATEWAY,
    StatusCode::SERVICE_UNAVAILABLE,
    StatusCode::GATEWAY_TIMEOUT,
    StatusCode::INSUFFICIENT_STORAGE,
    StatusCode::REQUEST_TIMEOUT,
    StatusCode::CONFLICT,
    StatusCode::TOO_MANY_REQUESTS,
];

/// Produce duration to sleep for (will never exceed 20 seconds).
fn backoff(attempt: u16) -> Duration {
    let seconds = std::cmp::min(2u64.pow(attempt as u32), 20);
    Duration::from_secs(seconds)
}
