use camino::Utf8PathBuf;
use dicom::object::{FileDicomObject, InMemDicomObject};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};

use crate::error::ChrisPacsError;

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
        obj: FileDicomObject<InMemDicomObject>,
        sop_instance_uid: &str,
    ) -> Result<(), ChrisPacsError> {
        let dst = self
            .dir
            .join(format!("{}.dcm", sop_instance_uid.trim_end_matches('\0')));
        if let Some(parent) = dst.parent() {
            fs_err::tokio::create_dir_all(parent).await?;
        }

        tokio::task::spawn_blocking(move || {
            obj.write_to_file(dst)
        }).await??;

        Ok(())
    }
}
