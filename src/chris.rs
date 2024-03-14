use camino::Utf8PathBuf;
use dicom::object::DefaultDicomObject;

use crate::error::{check, ChrisPacsError};
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
        // TODO implement debounce, retries, sleep
        let res = self
            .client
            .post(&self.url)
            .basic_auth(&self.username, Some(&self.password))
            .header(reqwest::header::ACCEPT, "application/json")
            .json(file)
            .send()?;
        let data = check(res)?.json()?;
        Ok(data)
    }
}
