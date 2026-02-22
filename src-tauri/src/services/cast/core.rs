use super::dlna::DlnaService;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CastProtocol {
    Auto,
    Sony,
    Dlna,
    Chromecast,
    Airplay,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CastDeviceInfo {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub available: bool,
    pub note: Option<String>,
}

fn is_sony_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("sony") || lower.contains("bravia")
}

pub async fn discover_cast_devices(protocol: CastProtocol, timeout_secs: u64) -> Result<Vec<CastDeviceInfo>, String> {
    match protocol {
        CastProtocol::Auto | CastProtocol::Sony | CastProtocol::Dlna => {
            let mut devices = DlnaService::discover_devices(timeout_secs).await?;

            if matches!(protocol, CastProtocol::Sony) {
                devices.retain(|d| is_sony_name(&d.name));
            }

            if matches!(protocol, CastProtocol::Auto) {
                devices.sort_by_key(|d| if is_sony_name(&d.name) { 0 } else { 1 });
            }

            Ok(devices
                .into_iter()
                .map(|d| {
                    let sony = is_sony_name(&d.name);
                    CastDeviceInfo {
                        id: d.name.clone(),
                        name: d.name,
                        protocol: "dlna".to_string(),
                        available: true,
                        note: if sony {
                            Some("Sony 推荐：已启用稳态参数".to_string())
                        } else if matches!(protocol, CastProtocol::Auto) {
                            Some("自动模式：非 Sony 设备使用通用 DLNA 参数".to_string())
                        } else {
                            None
                        },
                    }
                })
                .collect())
        }
        CastProtocol::Chromecast => Ok(vec![CastDeviceInfo {
            id: "chromecast-not-implemented".to_string(),
            name: "Chromecast (待实现)".to_string(),
            protocol: "chromecast".to_string(),
            available: false,
            note: Some("当前版本优先稳定支持 Sony DLNA，Chromecast 通道预留中".to_string()),
        }]),
        CastProtocol::Airplay => Ok(vec![CastDeviceInfo {
            id: "airplay-not-implemented".to_string(),
            name: "AirPlay (待实现)".to_string(),
            protocol: "airplay".to_string(),
            available: false,
            note: Some("当前版本优先稳定支持 Sony DLNA，AirPlay 通道预留中".to_string()),
        }]),
    }
}

pub async fn cast_media(
    service: &DlnaService,
    protocol: CastProtocol,
    device_id: String,
    video_url: String,
    title: String,
) -> Result<(), String> {
    match protocol {
        CastProtocol::Auto | CastProtocol::Sony | CastProtocol::Dlna => {
            service.cast_to_device(device_id, video_url, title).await
        }
        CastProtocol::Chromecast => Err("Chromecast casting is not implemented yet in this build".to_string()),
        CastProtocol::Airplay => Err("AirPlay casting is not implemented yet in this build".to_string()),
    }
}

pub async fn stop_cast_playback(
    service: &DlnaService,
    protocol: CastProtocol,
    device_id: String,
) -> Result<(), String> {
    match protocol {
        CastProtocol::Auto | CastProtocol::Sony | CastProtocol::Dlna => service.stop_playback(device_id).await,
        CastProtocol::Chromecast | CastProtocol::Airplay => Ok(()),
    }
}
