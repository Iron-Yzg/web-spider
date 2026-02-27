use crate::services::{
    DlnaService,
    CastProtocol,
    CastDeviceInfo,
    discover_cast_devices as discover_cast_devices_core,
    cast_media as cast_media_core,
    stop_cast_playback as stop_cast_playback_core,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use warp::Filter;

static DLNA_SERVICE: once_cell::sync::Lazy<Arc<Mutex<DlnaService>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(DlnaService::new())));
static CONTROL_SERVER: once_cell::sync::Lazy<Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));
static CONTROL_PORT: once_cell::sync::Lazy<Arc<Mutex<Option<u16>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));
static CONTROL_APP_HANDLE: once_cell::sync::Lazy<Arc<Mutex<Option<tauri::AppHandle>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastPlaylistItem {
    pub id: String,
    pub title: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CastRemoteSession {
    session_id: String,
    device_id: String,
    items: Vec<CastPlaylistItem>,
    current_index: usize,
    is_loading: bool,
    is_paused: bool,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CastRemoteState {
    session_id: String,
    device_id: String,
    current_index: usize,
    items: Vec<CastPlaylistItem>,
    is_loading: bool,
    is_paused: bool,
    last_error: Option<String>,
}

static CAST_REMOTE_SESSIONS: once_cell::sync::Lazy<Arc<Mutex<std::collections::HashMap<String, CastRemoteSession>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(std::collections::HashMap::new())));

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
    app_handle: tauri::AppHandle,
    file_path: String,
    port: u16,
) -> Result<String, String> {
    let service = DLNA_SERVICE.lock().await;
    service.start_media_server_with_resolve(app_handle, file_path, port).await
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

fn remote_page_html() -> &'static str {
    include_str!("cast_remote_page.html")
}

async fn play_index(session_id: String, index: usize) -> Result<(), String> {
    let (device_id, item) = {
        let mut guard = CAST_REMOTE_SESSIONS.lock().await;
        let s = guard
            .get_mut(&session_id)
            .ok_or_else(|| "session not found".to_string())?;
        if s.items.is_empty() {
            return Err("empty playlist".to_string());
        }
        if index >= s.items.len() {
            return Err("index out of range".to_string());
        }
        s.current_index = index;
        s.is_loading = true;
        s.last_error = None;
        s.is_paused = false;
        (s.device_id.clone(), s.items[index].clone())
    };

    let app = CONTROL_APP_HANDLE
        .lock()
        .await
        .clone()
        .ok_or_else(|| "app handle missing".to_string())?;

    let service = DLNA_SERVICE.lock().await;
    let media_url = service
        .start_media_server_with_resolve(app, item.source.clone(), 0)
        .await?;
    let cast_res = service.cast_to_device(device_id, media_url, item.title).await;

    let mut guard = CAST_REMOTE_SESSIONS.lock().await;
    if let Some(s) = guard.get_mut(&session_id) {
        s.is_loading = false;
        if let Err(e) = cast_res {
            s.last_error = Some(e.clone());
            return Err(e);
        }
    }
    Ok(())
}

async fn ensure_remote_server() -> Result<u16, String> {
    if let Some(port) = *CONTROL_PORT.lock().await {
        return Ok(port);
    }

    let route_state = warp::path!("cast" / "api" / String / "state").and_then(|sid: String| async move {
        let guard = CAST_REMOTE_SESSIONS.lock().await;
        if let Some(s) = guard.get(&sid) {
            let resp = CastRemoteState {
                session_id: s.session_id.clone(),
                device_id: s.device_id.clone(),
                current_index: s.current_index,
                items: s.items.clone(),
                is_loading: s.is_loading,
                is_paused: s.is_paused,
                last_error: s.last_error.clone(),
            };
            Ok::<_, warp::Rejection>(warp::reply::json(&resp))
        } else {
            Ok(warp::reply::json(&serde_json::json!({"error":"session not found"})))
        }
    });

    let route_play = warp::path!("cast" / "api" / String / "play" / usize)
        .and(warp::post())
        .and_then(|sid: String, idx: usize| async move {
            match play_index(sid, idx).await {
                Ok(_) => Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({"ok":true}))),
                Err(e) => Ok(warp::reply::json(&serde_json::json!({"ok":false,"error":e}))),
            }
        });

    let route_next = warp::path!("cast" / "api" / String / "next")
        .and(warp::post())
        .and_then(|sid: String| async move {
            let idx = {
                let guard = CAST_REMOTE_SESSIONS.lock().await;
                match guard.get(&sid) {
                    Some(s) if !s.items.is_empty() => (s.current_index + 1) % s.items.len(),
                    Some(_) => 0,
                    None => return Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({"ok":false,"error":"session not found"}))),
                }
            };
            match play_index(sid, idx).await {
                Ok(_) => Ok(warp::reply::json(&serde_json::json!({"ok":true}))),
                Err(e) => Ok(warp::reply::json(&serde_json::json!({"ok":false,"error":e}))),
            }
        });

    let route_prev = warp::path!("cast" / "api" / String / "prev")
        .and(warp::post())
        .and_then(|sid: String| async move {
            let idx = {
                let guard = CAST_REMOTE_SESSIONS.lock().await;
                match guard.get(&sid) {
                    Some(s) if !s.items.is_empty() => {
                        if s.current_index == 0 { s.items.len() - 1 } else { s.current_index - 1 }
                    }
                    Some(_) => 0,
                    None => return Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({"ok":false,"error":"session not found"}))),
                }
            };
            match play_index(sid, idx).await {
                Ok(_) => Ok(warp::reply::json(&serde_json::json!({"ok":true}))),
                Err(e) => Ok(warp::reply::json(&serde_json::json!({"ok":false,"error":e}))),
            }
        });

    let route_toggle_pause = warp::path!("cast" / "api" / String / "toggle-pause")
        .and(warp::post())
        .and_then(|sid: String| async move {
            let (device_id, paused_now) = {
                let mut guard = CAST_REMOTE_SESSIONS.lock().await;
                let s = match guard.get_mut(&sid) {
                    Some(v) => v,
                    None => return Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({"ok":false,"error":"session not found"}))),
                };
                (s.device_id.clone(), s.is_paused)
            };
            let service = DLNA_SERVICE.lock().await;
            let result = if paused_now {
                service.resume_playback(device_id).await
            } else {
                service.pause_playback(device_id).await
            };
            match result {
                Ok(_) => {
                    let mut guard = CAST_REMOTE_SESSIONS.lock().await;
                    if let Some(s) = guard.get_mut(&sid) {
                        s.is_paused = !paused_now;
                    }
                    Ok(warp::reply::json(&serde_json::json!({"ok":true})))
                }
                Err(e) => Ok(warp::reply::json(&serde_json::json!({"ok":false,"error":e}))),
            }
        });

    let route_stop = warp::path!("cast" / "api" / String / "stop")
        .and(warp::post())
        .and_then(|sid: String| async move {
            let device_id = {
                let guard = CAST_REMOTE_SESSIONS.lock().await;
                guard.get(&sid).map(|s| s.device_id.clone())
            };
            if let Some(d) = device_id {
                let service = DLNA_SERVICE.lock().await;
                let _ = service.stop_playback(d).await;
            }
            Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({"ok":true})))
        });

    let route_page = warp::path!("cast" / "remote" / String).map(|_sid: String| warp::reply::html(remote_page_html()));

    let routes = route_page
        .or(route_state)
        .or(route_play)
        .or(route_next)
        .or(route_prev)
        .or(route_toggle_pause)
        .or(route_stop);

    let (addr, server) = warp::serve(routes).bind_ephemeral(([0, 0, 0, 0], 0));
    let port = addr.port();
    let handle = tokio::spawn(server);
    *CONTROL_SERVER.lock().await = Some(handle);
    *CONTROL_PORT.lock().await = Some(port);
    Ok(port)
}

#[tauri::command]
pub async fn create_cast_remote_session(
    app_handle: tauri::AppHandle,
    device_id: String,
    items: Vec<CastPlaylistItem>,
    current_index: usize,
) -> Result<String, String> {
    if items.is_empty() {
        return Err("playlist is empty".to_string());
    }
    *CONTROL_APP_HANDLE.lock().await = Some(app_handle);
    let port = ensure_remote_server().await?;
    let ip = DlnaService::get_local_ip().await?;

    let sid = uuid::Uuid::new_v4().to_string();
    CAST_REMOTE_SESSIONS.lock().await.insert(
        sid.clone(),
        CastRemoteSession {
            session_id: sid.clone(),
            device_id,
            items,
            current_index,
            is_loading: false,
            is_paused: false,
            last_error: None,
        },
    );

    Ok(format!("http://{}:{}/cast/remote/{}", ip, port, sid))
}
