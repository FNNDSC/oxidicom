use camino::Utf8PathBuf;
use dicom::object::DefaultDicomObject;
use reqwest::StatusCode;
use std::time::Duration;

use crate::error::{check, ChrisPacsError, RequestError};
use crate::pacs_file::{BadTag, PacsFileRegistration, PacsFileResponse};

pub struct ChrisPacsStorage {
    client: reqwest::blocking::Client,
    retries: u16,
    url: String,
    username: String,
    password: String,
    dir: Utf8PathBuf,
    pacs_name: Option<String>,
}

impl ChrisPacsStorage {
    pub fn new(
        url: String,
        username: String,
        password: String,
        dir: Utf8PathBuf,
        retries: u16,
        pacs_name: Option<String>,
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
            pacs_name,
        }
    }

    pub fn store(
        &self,
        pacs_name: &str,
        obj: DefaultDicomObject,
    ) -> Result<(PacsFileResponse, Vec<BadTag>), ChrisPacsError> {
        let pacs_name = self.pacs_name.as_deref().unwrap_or(pacs_name);
        let (pacs_file, bad_tags) = PacsFileRegistration::new(pacs_name.to_string(), &obj)?;
        let dst = self.dir.join(&pacs_file.path);
        if let Some(parent) = dst.parent() {
            fs_err::create_dir_all(parent)?;
        }
        obj.write_to_file(dst)?;
        self.register_file(&pacs_file).map(|res| (res, bad_tags))
    }

    fn register_file(
        &self,
        file: &PacsFileRegistration,
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
        file: &PacsFileRegistration,
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
