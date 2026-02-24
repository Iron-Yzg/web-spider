use std::collections::HashMap;
use std::sync::Arc;

use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};
use tokio::sync::Mutex;
use url::Url;
use warp::http::StatusCode;

fn infer_referer(url: &str) -> Option<&'static str> {
    let lower = url.to_lowercase();
    if lower.contains("bilibili.com") || lower.contains("bilivideo.com") || lower.contains("hdslb.com") {
        Some("https://www.bilibili.com/")
    } else {
        None
    }
}

fn browser_ua() -> &'static str {
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36"
}

async fn fetch_with_headers(url: &str) -> Result<reqwest::Response, reqwest::Error> {
    let client = reqwest::Client::builder().build()?;
    let mut req = client
        .get(url)
        .header(reqwest::header::USER_AGENT, browser_ua());
    if let Some(referer) = infer_referer(url) {
        req = req.header(reqwest::header::REFERER, referer);
        req = req.header(reqwest::header::ORIGIN, "https://www.bilibili.com");
    }
    req.send().await
}

async fn fetch_with_headers_and_range(url: &str, range: Option<&str>) -> Result<reqwest::Response, reqwest::Error> {
    let client = reqwest::Client::builder().build()?;
    let mut req = client
        .get(url)
        .header(reqwest::header::USER_AGENT, browser_ua());
    if let Some(referer) = infer_referer(url) {
        req = req.header(reqwest::header::REFERER, referer);
        req = req.header(reqwest::header::ORIGIN, "https://www.bilibili.com");
    }
    if let Some(r) = range {
        req = req.header(reqwest::header::RANGE, r);
    }
    req.send().await
}

#[derive(Default)]
pub struct HlsProxyState {
    targets: Arc<Mutex<HashMap<String, String>>>,
}

impl HlsProxyState {
    pub fn new() -> Self {
        Self {
            targets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn targets(&self) -> Arc<Mutex<HashMap<String, String>>> {
        self.targets.clone()
    }

    pub async fn clear(&self) {
        self.targets.lock().await.clear();
    }

    pub async fn insert_target(&self, id: String, target: String) {
        self.targets.lock().await.insert(id, target);
    }
}

fn is_playlist_url(url: &str) -> bool {
    url.to_lowercase().contains(".m3u8")
}

fn encode_for_query(input: &str) -> String {
    utf8_percent_encode(input, NON_ALPHANUMERIC).to_string()
}

fn decode_from_query(input: &str) -> Result<String, String> {
    percent_decode_str(input)
        .decode_utf8()
        .map(|s| s.to_string())
        .map_err(|e| format!("Invalid encoded url: {}", e))
}

fn to_proxy_path(target: &str, host: Option<&str>) -> String {
    let prefix = host.map(|h| format!("http://{}", h)).unwrap_or_default();
    if is_playlist_url(target) {
        format!("{}/hls/playlist?u={}", prefix, encode_for_query(target))
    } else {
        format!("{}/hls/asset?u={}", prefix, encode_for_query(target))
    }
}

fn resolve_url(base: &str, rel: &str) -> Option<String> {
    let base = Url::parse(base).ok()?;
    let joined = base.join(rel).ok()?;
    Some(joined.to_string())
}

fn rewrite_tag_uri(line: &str, playlist_url: &str, host: Option<&str>) -> String {
    let needle = "URI=\"";
    if let Some(start) = line.find(needle) {
        let value_start = start + needle.len();
        if let Some(end_rel) = line[value_start..].find('"') {
            let value_end = value_start + end_rel;
            let raw = &line[value_start..value_end];
            if let Some(abs) = resolve_url(playlist_url, raw) {
                let proxied = to_proxy_path(&abs, host);
                return format!("{}{}{}", &line[..value_start], proxied, &line[value_end..]);
            }
        }
    }
    line.to_string()
}

fn rewrite_playlist_content(playlist_url: &str, content: &str, host: Option<&str>) -> String {
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            out.push(String::new());
            continue;
        }
        if trimmed.starts_with('#') {
            if trimmed.contains("URI=\"") {
                out.push(rewrite_tag_uri(line, playlist_url, host));
            } else {
                out.push(line.to_string());
            }
            continue;
        }

        if let Some(abs) = resolve_url(playlist_url, trimmed) {
            out.push(to_proxy_path(&abs, host));
        } else {
            out.push(line.to_string());
        }
    }
    out.join("\n")
}

fn make_text_response(status: StatusCode, body: String) -> warp::reply::Response {
    warp::http::Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(body.into())
        .unwrap_or_else(|_| warp::http::Response::new("internal error".into()))
}

pub async fn proxy_playlist_handler_by_id(
    id_raw: String,
    targets: Arc<Mutex<HashMap<String, String>>>,
    host: Option<String>,
) -> Result<warp::reply::Response, warp::Rejection> {
    let id = id_raw.strip_suffix(".m3u8").unwrap_or(&id_raw).to_string();
    let target = {
        let guard = targets.lock().await;
        guard.get(&id).cloned()
    };

    let target = if let Some(t) = target {
        t
    } else {
        return Ok(make_text_response(
            StatusCode::NOT_FOUND,
            format!("playlist id not found: {}", id),
        ));
    };

    let response = match fetch_with_headers(&target).await {
        Ok(r) => r,
        Err(e) => {
            return Ok(make_text_response(
                StatusCode::BAD_GATEWAY,
                format!("failed to fetch playlist: {}", e),
            ))
        }
    };

    let status = response.status();
    if !status.is_success() {
        return Ok(make_text_response(
            StatusCode::BAD_GATEWAY,
            format!("upstream playlist status: {}", status),
        ));
    }

    let text = match response.text().await {
        Ok(v) => v,
        Err(e) => {
            return Ok(make_text_response(
                StatusCode::BAD_GATEWAY,
                format!("failed to read playlist body: {}", e),
            ))
        }
    };

    let rewritten = rewrite_playlist_content(&target, &text, host.as_deref());
    let reply = warp::http::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/vnd.apple.mpegurl")
        .header("Access-Control-Allow-Origin", "*")
        .body(rewritten.into())
        .unwrap_or_else(|_| warp::http::Response::new("internal error".into()));
    Ok(reply)
}

pub async fn proxy_asset_handler(
    query: HashMap<String, String>,
    range: Option<String>,
) -> Result<warp::reply::Response, warp::Rejection> {
    let encoded = if let Some(u) = query.get("u") {
        u
    } else {
        return Ok(make_text_response(
            StatusCode::BAD_REQUEST,
            "missing query param: u".to_string(),
        ));
    };

    let target = match decode_from_query(encoded) {
        Ok(v) => v,
        Err(e) => return Ok(make_text_response(StatusCode::BAD_REQUEST, e)),
    };

    let response = match fetch_with_headers_and_range(&target, range.as_deref()).await {
        Ok(r) => r,
        Err(e) => {
            return Ok(make_text_response(
                StatusCode::BAD_GATEWAY,
                format!("failed to fetch media asset: {}", e),
            ))
        }
    };

    let status = response.status();
    if !status.is_success() {
        return Ok(make_text_response(
            StatusCode::BAD_GATEWAY,
            format!("upstream media status: {}", status),
        ));
    }

    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();
    let content_range = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let content_length = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let body = match response.bytes().await {
        Ok(v) => v,
        Err(e) => {
            return Ok(make_text_response(
                StatusCode::BAD_GATEWAY,
                format!("failed to read media body: {}", e),
            ))
        }
    };

    let mut builder = warp::http::Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Accept-Ranges", "bytes")
        .header("Access-Control-Allow-Origin", "*")
        .header("TransferMode.DLNA.ORG", "Streaming");
    if let Some(cr) = content_range {
        builder = builder.header("Content-Range", cr);
    }
    if let Some(cl) = content_length {
        builder = builder.header("Content-Length", cl);
    }
    let reply = builder
        .body(body.to_vec().into())
        .unwrap_or_else(|_| warp::http::Response::new("internal error".into()));
    Ok(reply)
}

pub async fn proxy_media_handler_by_id(
    id: String,
    targets: Arc<Mutex<HashMap<String, String>>>,
    range: Option<String>,
) -> Result<warp::reply::Response, warp::Rejection> {
    let target = {
        let guard = targets.lock().await;
        guard.get(&id).cloned()
    };

    let target = if let Some(t) = target {
        t
    } else {
        return Ok(make_text_response(
            StatusCode::NOT_FOUND,
            format!("media id not found: {}", id),
        ));
    };

    let response = match fetch_with_headers_and_range(&target, range.as_deref()).await {
        Ok(r) => r,
        Err(e) => {
            return Ok(make_text_response(
                StatusCode::BAD_GATEWAY,
                format!("failed to fetch media: {}", e),
            ))
        }
    };

    let status = response.status();
    if !status.is_success() {
        return Ok(make_text_response(
            StatusCode::BAD_GATEWAY,
            format!("upstream media status: {}", status),
        ));
    }

    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("video/mp4")
        .to_string();
    let content_range = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let content_length = response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let body = match response.bytes().await {
        Ok(v) => v,
        Err(e) => {
            return Ok(make_text_response(
                StatusCode::BAD_GATEWAY,
                format!("failed to read media body: {}", e),
            ))
        }
    };

    let mut builder = warp::http::Response::builder()
        .status(status)
        .header("Content-Type", content_type)
        .header("Accept-Ranges", "bytes")
        .header("Access-Control-Allow-Origin", "*")
        .header("TransferMode.DLNA.ORG", "Streaming");
    if let Some(cr) = content_range {
        builder = builder.header("Content-Range", cr);
    }
    if let Some(cl) = content_length {
        builder = builder.header("Content-Length", cl);
    }
    let reply = builder
        .body(body.to_vec().into())
        .unwrap_or_else(|_| warp::http::Response::new("internal error".into()));
    Ok(reply)
}
