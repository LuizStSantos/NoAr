use axum::{
    extract::{Path, Query, State},
    http::{
        HeaderMap, HeaderValue,
        header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, HOST},
    },
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use tokio::fs;

use crate::{
    api::{routes::stream_file, state::AppState},
    db::handles,
    file::norm_abs_path,
    utils::{
        errors::ServiceError,
        xmltv::{
            DEFAULT_DAYS, XmltvChannel, XmltvOptions, build_xmltv, library::has_image_extension,
        },
    },
};

#[derive(Debug, Deserialize)]
pub struct XmltvQuery {
    days: Option<u32>,
    lang: Option<String>,
    tvg_id: Option<String>,
    base_url: Option<String>,
}

/// **XMLTV guide**
///
/// Public program guide for IPTV players (Kodi, Jellyfin, TiviMate, ...),
/// built from the playlists and the `.nfo` files in the program folders.
///
/// ```BASH
/// curl -X GET 'http://127.0.0.1:8787/xmltv/1/guide.xml?days=3&lang=pt'
/// ```
///
/// Optional query parameters:
///
/// * `days`: days from the start of today, 1 to 14 (default 7)
/// * `lang`: language code for titles and descriptions, e.g. `pt`
/// * `tvg_id`: channel id in the guide, matching the `tvg-id` in the M3U
///   (default `noar.<channel id>`)
/// * `base_url`: public address for image links, when a reverse proxy serves
///   NoAr under a sub path (e.g. `https://example.org/tv`)
pub async fn get_xmltv(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Query(query): Query<XmltvQuery>,
    headers: HeaderMap,
) -> Result<Response, ServiceError> {
    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::NotFound(format!("Channel {id} not found!")))?;

    let config = manager.config.read().await.clone();
    let name = manager.channel.lock().await.name.clone();
    let base_url = match query.base_url.as_deref() {
        Some(url) => valid_base_url(url)?,
        None => request_base_url(&headers),
    };
    let channel = XmltvChannel {
        id: query
            .tvg_id
            .map(|tvg_id| tvg_id.trim().to_string())
            .filter(|tvg_id| !tvg_id.is_empty())
            .unwrap_or_else(|| format!("noar.{id}")),
        name,
    };
    let options = XmltvOptions {
        days: query.days.unwrap_or(DEFAULT_DAYS),
        lang: query.lang.filter(|lang| is_language_code(lang)),
        image_base_url: format!("{base_url}/xmltv/{id}/images"),
    };

    let events = handles::select_schedule_events(&state.pool, id).await?;
    let xml = build_xmltv(&config, &channel, &events, &options).await?;

    Ok((
        [
            (CONTENT_TYPE, "application/xml; charset=utf-8"),
            (CACHE_CONTROL, "public, max-age=300"),
        ],
        xml,
    )
        .into_response())
}

/// **XMLTV image**
///
/// Artwork linked from the guide. Only image files inside the channel storage
/// are served.
///
/// ```BASH
/// curl -X GET 'http://127.0.0.1:8787/xmltv/1/images/Morning%20News/poster.jpg'
/// ```
pub async fn get_xmltv_image(
    State(state): State<AppState>,
    Path((id, image)): Path<(i32, String)>,
    headers: HeaderMap,
) -> Result<Response, ServiceError> {
    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::NotFound(format!("Channel {id} not found!")))?;

    let storage = manager.config.read().await.channel.storage.clone();
    let (path, _, _) = norm_abs_path(&storage, &image)?;
    let is_file = fs::metadata(&path)
        .await
        .is_ok_and(|metadata| metadata.is_file());

    if !is_file || !has_image_extension(&path) {
        return Err(ServiceError::NotFound("Image not found".to_string()));
    }

    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    let mut response = stream_file(&path, &headers).await?;
    let response_headers = response.headers_mut();

    // stream_file sends downloads; images should show inline.
    response_headers.remove(CONTENT_DISPOSITION);
    response_headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );

    if let Ok(content_type) = HeaderValue::from_str(mime.as_ref()) {
        response_headers.insert(CONTENT_TYPE, content_type);
    }

    Ok(response)
}

/// Address the client used to reach the server, honouring the usual reverse
/// proxy headers.
fn request_base_url(headers: &HeaderMap) -> String {
    let first_value = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let scheme = match first_value("x-forwarded-proto") {
        Some("https") => "https",
        _ => "http",
    };
    let host = first_value("x-forwarded-host")
        .or_else(|| first_value(HOST.as_str()))
        .filter(|host| is_host(host))
        .unwrap_or("localhost");

    format!("{scheme}://{host}")
}

fn is_host(host: &str) -> bool {
    host.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
}

fn valid_base_url(url: &str) -> Result<String, ServiceError> {
    let url = url.trim().trim_end_matches('/');
    let has_scheme = url.starts_with("http://") || url.starts_with("https://");
    let is_plain = url
        .chars()
        .all(|c| c.is_ascii_graphic() && !matches!(c, '"' | '<' | '>' | '\\' | '?' | '#'));

    if !has_scheme || !is_plain {
        return Err(ServiceError::BadRequest(
            "base_url must be an http(s) address".to_string(),
        ));
    }

    Ok(url.to_string())
}

fn is_language_code(lang: &str) -> bool {
    (2..=12).contains(&lang.len()) && lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_follows_proxy_headers_and_rejects_odd_hosts() {
        let headers = |pairs: &[(&'static str, &'static str)]| {
            let mut headers = HeaderMap::new();

            for (name, value) in pairs {
                headers.insert(*name, HeaderValue::from_static(value));
            }

            headers
        };

        assert_eq!(
            request_base_url(&headers(&[("host", "127.0.0.1:8787")])),
            "http://127.0.0.1:8787"
        );
        assert_eq!(
            request_base_url(&headers(&[
                ("host", "127.0.0.1:8787"),
                ("x-forwarded-host", "tv.example.org"),
                ("x-forwarded-proto", "https"),
            ])),
            "https://tv.example.org"
        );
        assert_eq!(
            request_base_url(&headers(&[("host", "evil\"><x")])),
            "http://localhost"
        );
        assert_eq!(
            valid_base_url("https://example.org/tv/").unwrap(),
            "https://example.org/tv"
        );
        assert!(valid_base_url("javascript:alert(1)").is_err());
        assert!(is_language_code("pt-BR"));
        assert!(!is_language_code("pt\"x"));
    }
}
