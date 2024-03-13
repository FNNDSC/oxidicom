use camino::Utf8PathBuf;
use dicom::object::DefaultDicomObject;
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};

use crate::error::{check, ChrisPacsError};
use crate::pacs_file::{PacsFileRegistration, PacsFileResponse};

pub struct ChrisPacsStorage {
    client: reqwest_middleware::ClientWithMiddleware,
    url: String,
    username: String,
    password: String,
    dir: Utf8PathBuf,
}

impl ChrisPacsStorage {
    pub fn new(
        url: String,
        username: String,
        password: String,
        dir: Utf8PathBuf,
        retries: u32,
    ) -> Self {
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(retries);
        let client = reqwest_middleware::ClientBuilder::new(reqwest::Client::new())
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();
        Self {
            url,
            client,
            username,
            password,
            dir,
        }
    }

    pub async fn store(
        &self,
        pacs_name: &str,
        obj: DefaultDicomObject,
    ) -> Result<PacsFileResponse, ChrisPacsError> {
        let pacs_file = PacsFileRegistration::new(pacs_name.to_string(), &obj)?;
        let dst = self.dir.join(&pacs_file.path);
        if let Some(parent) = dst.parent() {
            fs_err::tokio::create_dir_all(parent).await?;
        }
        write_to_storage(obj, dst).await??;
        self.register_file(&pacs_file).await
    }

    async fn register_file(
        &self,
        file: &PacsFileRegistration,
    ) -> Result<PacsFileResponse, ChrisPacsError> {
        let res = self
            .client
            .post(&self.url)
            .basic_auth(&self.username, Some(&self.password))
            .header(reqwest::header::ACCEPT, "application/json")
            .json(file)
            .send()
            .await?;
        let data = check(res).await?.json().await?;
        Ok(data)
    }
}

async fn write_to_storage(
    obj: DefaultDicomObject,
    path: Utf8PathBuf,
) -> Result<Result<(), dicom::object::WriteError>, tokio::task::JoinError> {
    tokio::task::spawn_blocking(move || obj.write_to_file(path)).await
}
