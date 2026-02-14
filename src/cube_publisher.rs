use std::time::Duration;

use crate::types::{AuthToken, CubeRegistrationParams, LoginParams};
use reqwest::header::AUTHORIZATION;
use reqwest::Error;
use reqwest::Response;
use tokio::sync::mpsc::UnboundedReceiver;

pub(crate) async fn cube_publisher(
    mut rx: UnboundedReceiver<CubeRegistrationParams>,
    cube_login_url: String,
    cube_chris_password: String,
    cube_series_url: String,
) -> Result<(), Error> {
    let client = reqwest::Client::new();

    let cube_chris_token = get_chris_token(&client, cube_login_url, cube_chris_password).await;
    if cube_chris_token == "" {
        // XXX TODO: error handling
        tracing::error!(msg = "unable to get chris token");
        return Ok(());
    }

    while let Some((series, ndicom)) = rx.recv().await {
        tracing::info!(
            msg = "rx.recv",
            series_instance_uid = series.SeriesInstanceUID,
            ndicom = ndicom,
        );
        let pacs_name = series.pacs_name.clone();
        let series_instance_uid = series.SeriesInstanceUID.clone();
        let dicom_info_with_ndicom = series.into_dicominfo_with_ndicom(ndicom);
        let res: Result<Response, Error> = client
            .post(&cube_series_url)
            .json(&dicom_info_with_ndicom)
            .header(AUTHORIZATION, format!("Token {cube_chris_token}"))
            .send()
            .await;

        match res {
            Ok(r) => {
                let status = r.status();
                let text = r.text().await?;
                tracing::info!(
                    pacs_name = pacs_name.as_str(),
                    SeriesInstanceUID = series_instance_uid,
                    status = status.to_string(),
                    text = text,
                );
            }
            Err(e) => {
                tracing::error!(
                    pacs_name = pacs_name.as_str(),
                    SeriesInstanceUID = series_instance_uid,
                    message = e.to_string(),
                );
            }
        }
    }
    Ok(())
}

pub async fn get_chris_token(
    client: &reqwest::Client,
    cube_login_url: String,
    cube_chris_password: String,
) -> String {
    let params = LoginParams {
        username: "chris".to_string(),
        password: cube_chris_password,
    };
    let res = client
        .post(&cube_login_url)
        .json(&params)
        .timeout(Duration::new(3, 0))
        .send()
        .await;

    match res {
        Ok(r) => {
            let token_json = r.json::<AuthToken>().await;
            match token_json {
                Ok(token_res) => {
                    tracing::info!(msg = format!("token: {x}", x = token_res.token));
                    return token_res.token;
                }
                Err(e) => {
                    tracing::warn!(msg = format!("unable to r.json: e: {e}"));
                    return "".to_string();
                }
            }
        }
        Err(e) => {
            tracing::warn!(msg = format!("unable to client.send: e: {e}"));
            return "".to_string();
        }
    }
}
