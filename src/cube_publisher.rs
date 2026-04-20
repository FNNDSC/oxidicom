use crate::types::{AuthToken, CubeRegistrationParams, LoginParams};
use reqwest::header::AUTHORIZATION;
use reqwest::Error;
use reqwest::Response;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{thread, time::Duration};
use tokio::sync::mpsc::UnboundedReceiver;

pub(crate) async fn cube_publisher(
    mut rx: UnboundedReceiver<CubeRegistrationParams>,
    cube_login_url: String,
    cube_chris_username: String,
    cube_chris_password: String,
    cube_chris_refresh_duration: i64,
    cube_series_url: String,
) -> Result<(), Error> {
    let client = reqwest::Client::new();

    let params = LoginParams {
        username: cube_chris_username,
        password: cube_chris_password,
    };

    let mut cube_chris_token = retry_get_chris_token(&client, &cube_login_url, &params).await;
    let mut cube_chris_token_ts = now_ts();

    while let Some((series, ndicom)) = rx.recv().await {
        tracing::info!(
            msg = "rx.recv",
            series_instance_uid = &series.SeriesInstanceUID,
            ndicom = ndicom,
        );
        let current_ts = now_ts();
        if current_ts - cube_chris_token_ts > cube_chris_refresh_duration {
            cube_chris_token = retry_get_chris_token(&client, &cube_login_url, &params).await;
            let current_ts = now_ts();
            cube_chris_token_ts = current_ts;
        }
        tracing::info!(
            msg = "to post",
            series_instance_uid = series.SeriesInstanceUID,
            cube_chris_token = cube_chris_token,
            cube_chris_token_ts = cube_chris_token_ts,
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

pub async fn retry_get_chris_token(
    client: &reqwest::Client,
    cube_login_url: &String,
    params: &LoginParams,
) -> String {
    let mut token;
    let mut count = 0;
    loop {
        token = get_chris_token(client, cube_login_url, params).await;
        if !token.is_empty() {
            return token;
        }
        tracing::info!(
            msg = "cube_publisher.retry_get_chris_token: to sleep",
            count = count
        );

        thread::sleep(Duration::from_secs(5));
        count += 1;
    }
}

fn now_ts() -> i64 {
    return SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should go forward")
        .as_secs() as i64;
}

pub async fn get_chris_token(
    client: &reqwest::Client,
    cube_login_url: &String,
    params: &LoginParams,
) -> String {
    let res = client
        .post(cube_login_url)
        .json(params)
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
