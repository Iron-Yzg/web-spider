//! 视频转码服务 - 将不兼容格式转为 HLS 流
//!
//! 支持实时转码 MKV/AVI/FLV 等格式为 HLS，供前端播放

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// 转码会话信息
#[derive(Debug)]
pub struct TranscodeSession {
    pub output_dir: PathBuf,
    pub process: Option<Child>,
    pub pid: Option<u32>,
    pub is_running: bool,
}

/// 转码会话管理器
pub struct TranscodeManager {
    sessions: Arc<Mutex<HashMap<String, TranscodeSession>>>,
}

impl TranscodeManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 获取转码输出目录
    fn get_transcode_dir() -> PathBuf {
        let temp_dir = std::env::temp_dir();
        temp_dir.join("web-spider-transcode")
    }

    /// 清理旧转码文件
    async fn cleanup_old_transcodes() {
        let transcode_dir = Self::get_transcode_dir();
        if !transcode_dir.exists() {
            return;
        }

        // 读取目录中的所有子目录
        if let Ok(entries) = tokio::fs::read_dir(&transcode_dir).await {
            let mut entries = entries;
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(metadata) = entry.metadata().await {
                    if let Ok(modified) = metadata.modified() {
                        let age = std::time::SystemTime::now()
                            .duration_since(modified)
                            .unwrap_or_default();
                        // 删除超过 24 小时的转码文件
                        if age > std::time::Duration::from_secs(24 * 3600) {
                            let _ = tokio::fs::remove_dir_all(entry.path()).await;
                            tracing::info!("[transcode] 清理旧转码目录: {:?}", entry.path());
                        }
                    }
                }
            }
        }
    }

    /// 启动转码
    pub async fn start_transcode(
        &self,
        session_id: String,
        input_path: String,
        ffmpeg_path: PathBuf,
    ) -> Result<String, String> {
        // 清理旧转码文件
        Self::cleanup_old_transcodes().await;

        // 检查输入文件是否存在
        if !std::path::Path::new(&input_path).exists() {
            return Err(format!("输入文件不存在: {}", input_path));
        }

        // 创建输出目录
        let transcode_dir = Self::get_transcode_dir();
        let session_dir = transcode_dir.join(&session_id);
        tokio::fs::create_dir_all(&session_dir)
            .await
            .map_err(|e| format!("创建转码目录失败: {}", e))?;

        let playlist_path = session_dir.join("playlist.m3u8");
        let segment_pattern = session_dir.join("segment_%03d.ts");

        // 检查是否已有转码在进行
        let mut sessions = self.sessions.lock().await;
        if let Some(existing) = sessions.get(&session_id) {
            if existing.is_running {
                // 检查 playlist 是否已生成
                if playlist_path.exists() {
                    tracing::info!("[transcode] 使用已有转码会话: {}", session_id);
                    return Ok(format!("{}/playlist.m3u8", session_dir.to_string_lossy()));
                }
            }
        }

        tracing::info!("[transcode] 开始转码 - 会话: {}, 输入: {}", session_id, input_path);

        // 启动 ffmpeg 转码（优化参数，快速启动）
        // 参数说明：
        // - threads 0: 使用所有 CPU 核心
        // - preset ultrafast: 最快编码速度（牺牲一点质量换取速度）
        // - tune zerolatency: 零延迟模式
        // - crf 28: 稍高的压缩率，更快编码
        // - maxrate/bufsize: 限制码率避免过大文件
        // - hls_time 6: 更小的分片（6秒），更快开始播放
        // - hls_list_size 6: 只保留最近6个分片（约36秒），减少内存占用
        // - start_number 0: 从0开始编号
        let child = Command::new(&ffmpeg_path)
            .args(&[
                "-fflags", "+discardcorrupt+fastseek",
                "-i", &input_path,
                "-threads", "0",
                "-c:v", "libx264",
                "-c:a", "aac",
                "-preset", "ultrafast",
                "-tune", "zerolatency",
                "-crf", "28",
                "-maxrate", "8M",
                "-bufsize", "16M",
                "-pix_fmt", "yuv420p",
                "-movflags", "+faststart",
                "-f", "hls",
                "-hls_time", "6",
                "-hls_list_size", "6",
                "-hls_start_number", "0",
                "-hls_segment_filename", &segment_pattern.to_string_lossy(),
                "-hls_flags", "independent_segments+omit_endlist",
                &playlist_path.to_string_lossy(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("启动 ffmpeg 失败: {}", e))?;

        let pid = child.id();

        // 创建转码会话
        let session = TranscodeSession {
            output_dir: session_dir.clone(),
            process: Some(child),
            pid,
            is_running: true,
        };

        sessions.insert(session_id.clone(), session);

        // 在后台监控转码进程
        let sessions_clone = self.sessions.clone();
        let session_id_clone = session_id.clone();
        let playlist_path_clone = playlist_path.clone();
        tokio::spawn(async move {
            // 等待一段时间让转码开始
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            // 检查 playlist 是否已生成
            let mut retries = 0;
            while retries < 30 {
                if playlist_path_clone.exists() {
                    tracing::info!("[transcode] playlist 已生成: {:?}", playlist_path_clone);
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                retries += 1;
            }

            // 等待进程结束
            let mut sessions = sessions_clone.lock().await;
            if let Some(session) = sessions.get_mut(&session_id_clone) {
                if let Some(ref mut process) = session.process {
                    let _ = process.wait().await;
                }
                session.is_running = false;
            }
            tracing::info!("[transcode] 转码进程结束: {}", session_id_clone);
        });

        // 等待 playlist 生成（最多等待60秒）
        let mut retries = 0;
        while retries < 60 {
            if playlist_path.exists() {
                // 检查文件内容是否有效（至少有3个分片或播放时长超过18秒）
                if let Ok(content) = tokio::fs::read_to_string(&playlist_path).await {
                    let segment_count = content.lines().filter(|l| l.ends_with(".ts")).count();
                    if segment_count >= 3 {
                        tracing::info!("[transcode] 转码已就绪，分片数: {}", segment_count);
                        return Ok(playlist_path.to_string_lossy().to_string());
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            retries += 1;
        }

        // 超时，停止转码并返回错误
        tracing::error!("[transcode] 等待 playlist 生成超时（60秒），转码可能失败");
        
        // 停止转码进程
        if let Some(pid) = pid {
            if cfg!(target_os = "windows") {
                let _ = std::process::Command::new("taskkill")
                    .args(["/F", "/T", "/PID", &pid.to_string()])
                    .output();
            } else {
                let _ = std::process::Command::new("pkill")
                    .args(["-9", "-P", &pid.to_string()])
                    .output();
                let _ = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .output();
            }
        }
        
        // 清理会话
        sessions.remove(&session_id);
        
        // 尝试读取错误信息
        let err_msg = if session_dir.join("playlist.m3u8").exists() {
            "转码超时，可能是文件损坏或不支持的编码格式".to_string()
        } else {
            "转码启动失败，请检查 ffmpeg 是否正常".to_string()
        };
        
        Err(err_msg)
    }

    /// 停止转码
    pub async fn stop_transcode(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().await;
        
        if let Some(session) = sessions.get_mut(session_id) {
            if session.is_running {
                // 杀死进程
                if let Some(pid) = session.pid {
                    tracing::info!("[transcode] 停止转码进程: {} (PID: {})", session_id, pid);
                    
                    if cfg!(target_os = "windows") {
                        let _ = std::process::Command::new("taskkill")
                            .args(["/F", "/T", "/PID", &pid.to_string()])
                            .output();
                    } else {
                        // 先杀子进程
                        let _ = std::process::Command::new("pkill")
                            .args(["-9", "-P", &pid.to_string()])
                            .output();
                        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                        // 再杀主进程
                        let _ = std::process::Command::new("kill")
                            .args(["-9", &pid.to_string()])
                            .output();
                    }
                }
                
                session.is_running = false;
            }
            
            // 清理转码文件
            if session.output_dir.exists() {
                let _ = tokio::fs::remove_dir_all(&session.output_dir).await;
                tracing::info!("[transcode] 清理转码目录: {:?}", session.output_dir);
            }
            
            sessions.remove(session_id);
        }
        
        Ok(())
    }

    /// 获取所有运行中的会话
    pub async fn get_running_sessions(&self) -> Vec<String> {
        let sessions = self.sessions.lock().await;
        sessions
            .iter()
            .filter(|(_, s)| s.is_running)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl Default for TranscodeManager {
    fn default() -> Self {
        Self::new()
    }
}

// 全局转码管理器实例
static TRANSCODE_MANAGER: std::sync::OnceLock<TranscodeManager> = std::sync::OnceLock::new();

/// 获取转码管理器
pub fn get_transcode_manager() -> &'static TranscodeManager {
    TRANSCODE_MANAGER.get_or_init(TranscodeManager::new)
}

/// 启动视频转码（Tauri 命令）
pub async fn start_video_transcode_cmd(
    app_handle: tauri::AppHandle,
    file_path: String,
    session_id: String,
) -> Result<String, String> {
    use crate::services::get_sidecar_path;
    
    let ffmpeg_path = get_sidecar_path(&app_handle, "ffmpeg")?;
    let manager = get_transcode_manager();
    
    manager.start_transcode(session_id, file_path, ffmpeg_path).await
}

/// 停止视频转码（Tauri 命令）
pub async fn stop_video_transcode_cmd(session_id: String) -> Result<(), String> {
    let manager = get_transcode_manager();
    manager.stop_transcode(&session_id).await
}

/// 清理所有转码会话（应用退出时调用）
pub async fn cleanup_all_transcodes() {
    let manager = get_transcode_manager();
    let sessions = manager.get_running_sessions().await;
    
    for session_id in sessions {
        let _ = manager.stop_transcode(&session_id).await;
    }
    
    // 清理临时目录
    let transcode_dir = TranscodeManager::get_transcode_dir();
    if transcode_dir.exists() {
        let _ = tokio::fs::remove_dir_all(&transcode_dir).await;
    }
}
