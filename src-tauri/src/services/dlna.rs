use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;
use warp::Filter;

#[derive(Debug, Clone)]
pub struct DlnaDevice {
    pub name: String,
    pub udn: String,
}

pub struct DlnaService {
    streaming_server: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl DlnaService {
    pub fn new() -> Self {
        Self {
            streaming_server: Arc::new(Mutex::new(None)),
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

        let host_ip = Self::get_local_ip().await?;
        let path_buf = std::path::PathBuf::from(&file_path);
        
        tracing::info!("[DLNA] Starting server with file: {:?}", path_buf);
        
        // 使用 warp 内置的 file 过滤，自动处理 Range 请求
        let video_route = warp::path("video")
            .and(warp::fs::file(path_buf))
            .map(|reply: warp::filters::fs::File| {
                warp::reply::with_header(
                    warp::reply::with_header(
                        warp::reply::with_header(
                            reply,
                            "Content-Type", "video/mp4"
                        ),
                        "TransferMode.DLNA.ORG", "Streaming"
                    ),
                    "ContentFeatures.DLNA.ORG", "DLNA.ORG_OP=01;DLNA.ORG_PS=1;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000"
                )
            });

        let addr: SocketAddr = format!("{}:{}", host_ip, port).parse()
            .map_err(|e| format!("Failed to parse address: {}", e))?;

        tracing::info!("[DLNA] Binding to {}", addr);

        let (addr, server) = warp::serve(video_route).bind_ephemeral(addr);

        let handle = tokio::spawn(server);

        *self.streaming_server.lock().await = Some(handle);

        let streaming_url = format!("http://{}:{}/video", host_ip, addr.port());
        tracing::info!("[DLNA] Media server started at {}", streaming_url);
        
        Ok(streaming_url)
    }

    pub async fn stop_media_server(&self) -> Result<(), String> {
        if let Some(handle) = self.streaming_server.lock().await.take() {
            handle.abort();
        }
        Ok(())
    }

    pub async fn stop_playback(&self, device_name: String) -> Result<(), String> {
        tracing::info!("[DLNA] Stop playback on device: {}", device_name);
        
        let spec = crab_dlna::RenderSpec::Query(5u64, device_name.clone());
        
        let render = crab_dlna::Render::new(spec)
            .await
            .map_err(|e| format!("Failed to create render: {:?}", e))?;

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
        _title: String,
    ) -> Result<(), String> {
        let spec = crab_dlna::RenderSpec::Query(5u64, device_name.clone());
        
        let render = crab_dlna::Render::new(spec)
            .await
            .map_err(|e| format!("Failed to create render: {:?}", e))?;

        let stream_url = video_url.trim_end_matches("/").to_string();
        
        tracing::info!("[DLNA] Cast to {} at {}", device_name, stream_url);

        let service = &render.service;
        let device_url = render.device.url();

        // 使用完整的 protocolInfo（索尼电视需要）
        let metadata = format!(
            r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/" xmlns:dlna="urn:schemas-dlna-org:metadata-1-0/">
  <item id="0" parentID="-1" restricted="1">
    <dc:title>Video</dc:title>
    <res protocolInfo="http-get:*:video/mp4:DLNA.ORG_PN=AVC_MP4_HP_HD_24p;DLNA.ORG_OP=01;DLNA.ORG_CI=0;DLNA.ORG_FLAGS=01700000000000000000000000000000">{}</res>
    <upnp:class>object.item.videoItem.movie</upnp:class>
  </item>
</DIDL-Lite>"#,
            stream_url
        );
        
        let set_args = format!(
            "<InstanceID>0</InstanceID><CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
            stream_url, metadata
        );
        
        tracing::info!("[DLNA] SetAVTransportURI args: {}", set_args);
        
        let set_result = service
            .action(device_url, "SetAVTransportURI", &set_args)
            .await;

        match set_result {
            Ok(_) => {
                tracing::info!("[DLNA] SetAVTransportURI success, sending Play...");
                
                tokio::time::sleep(Duration::from_millis(500)).await;
                
                let play_args = "<InstanceID>0</InstanceID><Speed>1</Speed>";
                let _play_result = service
                    .action(device_url, "Play", play_args)
                    .await;
                
                tracing::info!("[DLNA] Play command sent!");
            }
            Err(e) => {
                tracing::error!("[DLNA] SetAVTransportURI failed: {:?}", e);
                return Err(format!("DLNA command failed: {:?}", e));
            }
        }
        
        Ok(())
    }
}

impl Default for DlnaService {
    fn default() -> Self {
        Self::new()
    }
}
