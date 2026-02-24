use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use warp::Filter;

use super::hls_proxy::{
    HlsProxyState,
    proxy_media_handler_by_id,
    proxy_asset_handler,
    proxy_playlist_handler_by_id,
};

#[derive(Debug, Clone)]
pub struct DlnaDevice {
    pub name: String,
    pub udn: String,
}

pub struct DlnaService {
    streaming_server: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    hls_proxy: HlsProxyState,
}

#[derive(Debug, Clone, Copy)]
enum DlnaProfile {
    Sony,
    Generic,
}

impl DlnaService {
    pub fn new() -> Self {
        Self {
            streaming_server: Arc::new(Mutex::new(None)),
            hls_proxy: HlsProxyState::new(),
        }
    }

    pub async fn discover_devices(timeout_secs: u64) -> Result<Vec<DlnaDevice>, String> {
        let devices = crab_dlna::Render::discover(timeout_secs)
            .await
            .map_err(|e| format!("Discovery failed: {:?}", e))?;

        let mut result = Vec::new();
        for d in devices {
            result.push(DlnaDevice {
                name: d.device.friendly_name().to_string(),
                udn: d.device.url().to_string(),
            });
        }

        Ok(result)
    }

    fn infer_profile(name: &str) -> DlnaProfile {
        let lower = name.to_lowercase();
        if lower.contains("sony") || lower.contains("bravia") {
            DlnaProfile::Sony
        } else {
            DlnaProfile::Generic
        }
    }

    fn escape_xml(input: &str) -> String {
        input
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    fn is_http_url(url: &str) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
    }

    fn is_playlist_url(url: &str) -> bool {
        url.to_lowercase().contains(".m3u8")
    }

    fn is_direct_stream_url(url: &str) -> bool {
        let lower = url.to_lowercase();
        lower.contains(".m3u8")
            || lower.contains(".mp4")
            || lower.contains(".mkv")
            || lower.contains(".webm")
            || lower.contains(".mov")
            || lower.contains(".avi")
            || lower.contains(".flv")
            || lower.contains(".wmv")
    }

    async fn resolve_cast_source(
        &self,
        app_handle: &tauri::AppHandle,
        source: String,
    ) -> Result<String, String> {
        let normalized = source.trim().replace("\\/", "/");
        if Self::is_http_url(&normalized) && !Self::is_direct_stream_url(&normalized) {
            tracing::info!("[DLNA] Detected page url, extracting stream via yt-dlp: {}", normalized);
            return crate::services::get_cast_stream_url(app_handle, &normalized).await;
        }
        Ok(normalized)
    }

    pub async fn start_media_server_with_resolve(
        &self,
        app_handle: tauri::AppHandle,
        source: String,
        port: u16,
    ) -> Result<String, String> {
        let resolved = self.resolve_cast_source(&app_handle, source).await?;
        self.start_media_server(resolved, port).await
    }


    fn mime_and_protocol(stream_url: &str, profile: DlnaProfile) -> (String, String) {
        let lower = stream_url.to_lowercase();
        let is_m3u8 = lower.contains(".m3u8");

        if is_m3u8 {
            (
                "application/vnd.apple.mpegurl".to_string(),
                "http-get:*:application/vnd.apple.mpegurl:DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000".to_string(),
            )
        } else {
            match profile {
                DlnaProfile::Sony => (
                    "video/mp4".to_string(),
                    "http-get:*:video/mp4:DLNA.ORG_PN=AVC_MP4_HP_HD_24p;DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000".to_string(),
                ),
                DlnaProfile::Generic => (
                    "video/mp4".to_string(),
                    "http-get:*:video/mp4:DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000".to_string(),
                ),
            }
        }
    }

    async fn resolve_render(
        device_name: &str,
        timeout_secs: u64,
    ) -> Result<crab_dlna::Render, String> {
        // First attempt: query by the provided display name (existing behavior)
        let spec = crab_dlna::RenderSpec::Query(timeout_secs, device_name.to_string());
        if let Ok(render) = crab_dlna::Render::new(spec).await {
            return Ok(render);
        }

        // Fallback: rediscover and fuzzy-match by friendly name.
        let renders = crab_dlna::Render::discover(timeout_secs)
            .await
            .map_err(|e| format!("Discovery failed during resolve: {:?}", e))?;

        let target = device_name.to_lowercase();
        let exact = renders.iter().find(|r| r.device.friendly_name().eq(device_name));
        if let Some(r) = exact {
            return Ok(r.clone());
        }

        let contains = renders
            .iter()
            .find(|r| r.device.friendly_name().to_lowercase().contains(&target));
        if let Some(r) = contains {
            return Ok(r.clone());
        }

        renders
            .into_iter()
            .next()
            .ok_or_else(|| format!("No render found for device: {}", device_name))
    }

    pub async fn get_local_ip() -> Result<String, String> {
        local_ip_address::local_ip()
            .map(|ip| ip.to_string())
            .map_err(|e| format!("Failed to get local IP: {}", e))
    }

    pub async fn start_media_server(
        &self,
        file_path: String,
        port: u16,
    ) -> Result<String, String> {
        self.stop_media_server().await?;
        self.hls_proxy.clear().await;

        let host_ip = Self::get_local_ip().await?;
        let normalized = file_path.trim().replace("\\/", "/");
        let is_remote_http = Self::is_http_url(&normalized);
        let is_remote_hls = is_remote_http && Self::is_playlist_url(&normalized);
        tracing::info!("[DLNA] Starting media server for source: {}", normalized);

        let bind_port = if port == 0 { 0 } else { port };
        let bind_addr = ([0, 0, 0, 0], bind_port);
        tracing::info!("[DLNA] Binding to 0.0.0.0:{}", bind_port);

        let streaming_url = if is_remote_hls {
            let id = uuid::Uuid::new_v4().to_string();
            self.hls_proxy.insert_target(id.clone(), normalized.clone()).await;

            let targets = self.hls_proxy.targets();
            let playlist_route = warp::path!("hls" / "playlist" / String)
                .and(warp::any().map(move || targets.clone()))
                .and_then(proxy_playlist_handler_by_id);

            let asset_route = warp::path!("hls" / "asset")
                .and(warp::query::<std::collections::HashMap<String, String>>())
                .and_then(proxy_asset_handler);

            let routes = playlist_route.or(asset_route);
            let (addr, server) = warp::serve(routes).bind_ephemeral(bind_addr);
            let handle = tokio::spawn(server);
            *self.streaming_server.lock().await = Some(handle);

            let start_url = format!(
                "http://{}:{}/hls/playlist/{}.m3u8",
                host_ip,
                addr.port(),
                id
            );
            start_url
        } else if is_remote_http {
            let id = uuid::Uuid::new_v4().to_string();
            self.hls_proxy.insert_target(id.clone(), normalized.clone()).await;

            let targets = self.hls_proxy.targets();
            let media_route = warp::path!("proxy" / "media" / String)
                .and(warp::any().map(move || targets.clone()))
                .and_then(proxy_media_handler_by_id);

            let (addr, server) = warp::serve(media_route).bind_ephemeral(bind_addr);
            let handle = tokio::spawn(server);
            *self.streaming_server.lock().await = Some(handle);

            format!("http://{}:{}/proxy/media/{}", host_ip, addr.port(), id)
        } else {
            let path_buf = std::path::PathBuf::from(&normalized);
            let video_route = warp::path("video")
                .and(warp::fs::file(path_buf))
                .map(|reply: warp::filters::fs::File| {
                    warp::reply::with_header(
                        warp::reply::with_header(
                            warp::reply::with_header(
                                warp::reply::with_header(reply, "Content-Type", "video/mp4"),
                                "Accept-Ranges",
                                "bytes",
                            ),
                            "TransferMode.DLNA.ORG",
                            "Streaming",
                        ),
                        "ContentFeatures.DLNA.ORG",
                        "DLNA.ORG_OP=01;DLNA.ORG_PS=1;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000",
                    )
                });
            let (addr, server) = warp::serve(video_route).bind_ephemeral(bind_addr);
            let handle = tokio::spawn(server);
            *self.streaming_server.lock().await = Some(handle);

            let start_url = format!("http://{}:{}/video", host_ip, addr.port());
            start_url
        };
        tracing::info!("[DLNA] Media server started at {}", streaming_url);

        Ok(streaming_url)
    }

    pub async fn stop_media_server(&self) -> Result<(), String> {
        if let Some(handle) = self.streaming_server.lock().await.take() {
            handle.abort();
        }
        self.hls_proxy.clear().await;
        Ok(())
    }

    pub async fn stop_playback(&self, device_name: String) -> Result<(), String> {
        tracing::info!("[DLNA] Stop playback on device: {}", device_name);

        let render = Self::resolve_render(&device_name, 5).await?;

        let service = &render.service;
        let device_url = render.device.url();
        
        let stop_args = "<InstanceID>0</InstanceID>";
        
        let result = service
            .action(device_url, "Stop", stop_args)
            .await;

        match result {
            Ok(_) => {
                tracing::info!("[DLNA] Stop command success");
            }
            Err(e) => {
                tracing::error!("[DLNA] Stop command failed: {:?}", e);
            }
        }
        
        self.stop_media_server().await?;
        
        Ok(())
    }

    pub async fn cast_to_device(
        &self,
        device_name: String,
        video_url: String,
        title: String,
    ) -> Result<(), String> {
        let render = Self::resolve_render(&device_name, 5).await?;

        let stream_url = video_url.trim_end_matches("/").to_string();

        let profile = Self::infer_profile(&render.device.friendly_name());
        tracing::info!(
            "[DLNA] Cast to {} ({:?}) at {}",
            device_name,
            profile,
            stream_url
        );

        let service = &render.service;
        let device_url = render.device.url();

        let (content_type, protocol_info) = Self::mime_and_protocol(&stream_url, profile);
        let escaped_current_uri = Self::escape_xml(&stream_url);
        let escaped_title = Self::escape_xml(&title);
        let escaped_res_url = Self::escape_xml(&stream_url);
        let metadata_xml = format!(
            r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns:dlna="urn:schemas-dlna-org:metadata-1-0/">
  <item id="0" parentID="-1" restricted="1">
    <dc:title>{}</dc:title>
    <upnp:class>object.item.videoItem.movie</upnp:class>
    <upnp:mimeType>{}</upnp:mimeType>
    <res protocolInfo="{}">{}</res>
  </item>
</DIDL-Lite>"#,
            escaped_title, content_type, protocol_info, escaped_res_url
        );

        let full_metadata_arg = Self::escape_xml(&metadata_xml);
        let empty_metadata_arg = String::new();

        let using_local_hls_proxy = stream_url.contains("/hls/playlist/") && stream_url.to_lowercase().contains(".m3u8");
        let is_hls = stream_url.to_lowercase().contains(".m3u8");

        let set_args_with_full = format!(
            "<InstanceID>0</InstanceID><CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
            escaped_current_uri, full_metadata_arg
        );

        let set_args_with_empty = format!(
            "<InstanceID>0</InstanceID><CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
            escaped_current_uri, empty_metadata_arg
        );

        // For Sony: local/proxy HLS and local MP4 generally prefer full metadata; remote HLS sometimes prefers empty.
        // Keep both as fallback and swap order by scenario.
        let (set_args_primary, set_args_fallback) = if matches!(profile, DlnaProfile::Sony) && is_hls && !using_local_hls_proxy {
            (set_args_with_empty, set_args_with_full)
        } else {
            (set_args_with_full, set_args_with_empty)
        };

        let set_args_primary = if set_args_primary.is_empty() {
            format!(
                "<InstanceID>0</InstanceID><CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
                escaped_current_uri, empty_metadata_arg
            )
        } else {
            set_args_primary
        };

        tracing::info!("[DLNA] SetAVTransportURI primary args: {}", set_args_primary);

        // Sony TVs are more sensitive. Use retry with a small delay and explicit Stop before Set.
        let stop_args = "<InstanceID>0</InstanceID>";
        let _ = service.action(device_url, "Stop", stop_args).await;

        let mut last_err: Option<String> = None;
        for attempt in 1..=3 {
            let set_payload = if attempt == 1 {
                &set_args_primary
            } else {
                &set_args_fallback
            };
            let set_result = service.action(device_url, "SetAVTransportURI", set_payload).await;
            match set_result {
                Ok(_) => {
                    tracing::info!("[DLNA] SetAVTransportURI success (attempt {}), sending Play...", attempt);
                    tokio::time::sleep(Duration::from_millis(600)).await;

                    let play_args = "<InstanceID>0</InstanceID><Speed>1</Speed>";
                    match service.action(device_url, "Play", play_args).await {
                        Ok(_) => {
                            tracing::info!("[DLNA] Play command success");
                            return Ok(());
                        }
                        Err(e) => {
                            last_err = Some(format!("Play failed on attempt {}: {:?}", attempt, e));
                        }
                    }
                }
                Err(e) => {
                    last_err = Some(format!("SetAVTransportURI failed on attempt {}: {:?}", attempt, e));
                }
            }

            tokio::time::sleep(Duration::from_millis(450)).await;
        }

        Err(last_err.unwrap_or_else(|| "DLNA cast failed with unknown error".to_string()))
    }
}

impl Default for DlnaService {
    fn default() -> Self {
        Self::new()
    }
}
