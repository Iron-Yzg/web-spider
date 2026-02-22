use crate::services::{
    DlnaService,
    CastProtocol,
    CastDeviceInfo,
    discover_cast_devices as discover_cast_devices_core,
    cast_media as cast_media_core,
    stop_cast_playback as stop_cast_playback_core,
};
use std::sync::Arc;
use tokio::sync::Mutex;

static DLNA_SERVICE: once_cell::sync::Lazy<Arc<Mutex<DlnaService>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(DlnaService::new())));

#[derive(serde::Serialize)]
pub struct DlnaDeviceInfo {
    pub name: String,
    pub udn: String,
}

#[tauri::command]
pub async fn discover_dlna_devices(timeout_secs: u64) -> Result<Vec<DlnaDeviceInfo>, String> {
    let devices = DlnaService::discover_devices(timeout_secs).await?;
    Ok(devices
        .into_iter()
        .map(|d| DlnaDeviceInfo {
            name: d.name,
            udn: d.udn,
        })
        .collect())
}

#[tauri::command]
pub async fn discover_cast_devices(
    protocol: CastProtocol,
    timeout_secs: u64,
) -> Result<Vec<CastDeviceInfo>, String> {
    discover_cast_devices_core(protocol, timeout_secs).await
}

#[tauri::command]
pub async fn get_local_ip_address() -> Result<String, String> {
    DlnaService::get_local_ip().await
}

#[tauri::command]
pub async fn start_dlna_media_server(
    file_path: String,
    port: u16,
) -> Result<String, String> {
    let service = DLNA_SERVICE.lock().await;
    service.start_media_server(file_path, port).await
}

#[tauri::command]
pub async fn stop_dlna_media_server() -> Result<(), String> {
    let service = DLNA_SERVICE.lock().await;
    service.stop_media_server().await
}

#[tauri::command]
pub async fn stop_dlna_playback(device_name: String) -> Result<(), String> {
    let service = DLNA_SERVICE.lock().await;
    service.stop_playback(device_name).await
}

#[tauri::command]
pub async fn cast_to_dlna_device(
    device_name: String,
    video_url: String,
    title: String,
) -> Result<(), String> {
    let service = DLNA_SERVICE.lock().await;
    service.cast_to_device(device_name, video_url, title).await
}

#[tauri::command]
pub async fn cast_media(
    protocol: CastProtocol,
    device_id: String,
    video_url: String,
    title: String,
) -> Result<(), String> {
    let service = DLNA_SERVICE.lock().await;
    cast_media_core(&service, protocol, device_id, video_url, title).await
}

#[tauri::command]
pub async fn stop_cast_playback(protocol: CastProtocol, device_id: String) -> Result<(), String> {
    let service = DLNA_SERVICE.lock().await;
    stop_cast_playback_core(&service, protocol, device_id).await
}
