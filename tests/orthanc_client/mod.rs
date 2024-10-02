/// Tell Orthanc to push a DICOM series.
pub async fn orthanc_store(
    orthanc_url: &str,
    push_to: &str,
    series_instance_uid: &str,
) -> Result<StoreResponse, reqwest::Error> {
    let client = OrthancClient::new(orthanc_url);
    client.store_series(push_to, series_instance_uid).await
}

struct OrthancClient<'a> {
    client: reqwest::Client,
    url: &'a str,
}

impl<'a> OrthancClient<'a> {
    fn new(url: &'a str) -> Self {
        Self {
            url,
            client: reqwest::Client::new(),
        }
    }

    async fn store_series(
        &self,
        aet: &str,
        series_instance_uid: &str,
    ) -> Result<StoreResponse, reqwest::Error> {
        let resources = self.find_series(series_instance_uid).await?;
        self.store(aet, resources).await
    }

    async fn find_series(&self, series_instance_uid: &str) -> Result<Vec<String>, reqwest::Error> {
        let body = OrthancFind {
            level: "Series",
            limit: 1,
            query: SeriesQuery {
                SeriesInstanceUID: series_instance_uid,
            },
        };
        self.client
            .post(format!("{}/tools/find", self.url))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    }

    async fn store(
        &self,
        aet: &str,
        resources: Vec<String>,
    ) -> Result<StoreResponse, reqwest::Error> {
        let body = StoreRequest {
            synchronous: true,
            resources,
            timeout: 60,
        };
        self.client
            .post(format!("{}/modalities/{}/store", self.url, aet))
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct OrthancFind<'a> {
    level: &'a str,
    limit: usize,
    query: SeriesQuery<'a>,
}

#[derive(serde::Serialize)]
#[allow(non_snake_case)]
struct SeriesQuery<'a> {
    SeriesInstanceUID: &'a str,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct StoreRequest {
    synchronous: bool,
    resources: Vec<String>,
    timeout: u32,
}

#[allow(unused)]
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StoreResponse {
    pub description: String,
    pub failed_instances_count: usize,
    pub instances_count: usize,
    pub local_aet: String,
    pub parent_resources: Vec<String>,
    pub remote_aet: String,
}
