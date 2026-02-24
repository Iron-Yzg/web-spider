use crate::services::{start_video_playback, stop_video_transcode_cmd};

#[tauri::command]
pub async fn stop_video_transcode(session_id: String) -> Result<(), String> {
    tracing::info!("[commands] 停止视频转码: session={}", session_id);
    stop_video_transcode_cmd(session_id).await
}

/// 启动视频播放（自动选择解复用或转码）
#[tauri::command]
pub async fn start_video_playback_cmd(
    app_handle: tauri::AppHandle,
    file_path: String,
    session_id: String,
) -> Result<(String, bool), String> {
    tracing::info!("[commands] 开始视频播放: session={}, path={}", session_id, file_path);
    start_video_playback(app_handle, file_path, session_id).await
}

/// 使用系统播放器打开视频文件
#[tauri::command]
pub async fn open_with_system_player(app_handle: tauri::AppHandle, file_path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    tracing::info!("[commands] 使用系统播放器打开: {}", file_path);

    app_handle
        .opener()
        .open_path(&file_path, None::<&str>)
        .map_err(|e| format!("打开视频失败: {}", e))?;

    Ok(())
}
