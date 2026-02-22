use std::collections::HashMap;
use std::sync::Arc;

use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};
use tokio::sync::Mutex;
use url::Url;
use warp::http::StatusCode;

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

fn to_proxy_path(target: &str) -> String {
    if is_playlist_url(target) {
        format!("/hls/playlist?u={}", encode_for_query(target))
    } else {
        format!("/hls/asset?u={}", encode_for_query(target))
    }
}

fn resolve_url(base: &str, rel: &str) -> Option<String> {
    let base = Url::parse(base).ok()?;
    let joined = base.join(rel).ok()?;
    Some(joined.to_string())
}

fn rewrite_tag_uri(line: &str, playlist_url: &str) -> String {
    let needle = "URI=\"";
    if let Some(start) = line.find(needle) {
        let value_start = start + needle.len();
        if let Some(end_rel) = line[value_start..].find('"') {
            let value_end = value_start + end_rel;
            let raw = &line[value_start..value_end];
            if let Some(abs) = resolve_url(playlist_url, raw) {
                let proxied = to_proxy_path(&abs);
                return format!("{}{}{}", &line[..value_start], proxied, &line[value_end..]);
            }
        }
    }
    line.to_string()
}

fn rewrite_playlist_content(playlist_url: &str, content: &str) -> String {
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            out.push(String::new());
            continue;
        }
        if trimmed.starts_with('#') {
            if trimmed.contains("URI=\"") {
                out.push(rewrite_tag_uri(line, playlist_url));
            } else {
                out.push(line.to_string());
            }
            continue;
        }

        if let Some(abs) = resolve_url(playlist_url, trimmed) {
            out.push(to_proxy_path(&abs));
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

    let response = match reqwest::get(&target).await {
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

    let rewritten = rewrite_playlist_content(&target, &text);
    let reply = warp::http::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/vnd.apple.mpegurl")
        .header("Access-Control-Allow-Origin", "*")
        .body(rewritten.into())
        .unwrap_or_else(|_| warp::http::Response::new("internal error".into()));
    Ok(reply)
}

pub async fn proxy_asset_handler(query: HashMap<String, String>) -> Result<warp::reply::Response, warp::Rejection> {
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

    let response = match reqwest::get(&target).await {
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

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let body = match response.bytes().await {
        Ok(v) => v,
        Err(e) => {
            return Ok(make_text_response(
                StatusCode::BAD_GATEWAY,
                format!("failed to read media body: {}", e),
            ))
        }
    };

    let reply = warp::http::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header("Access-Control-Allow-Origin", "*")
        .body(body.to_vec().into())
        .unwrap_or_else(|_| warp::http::Response::new("internal error".into()));
    Ok(reply)
}
