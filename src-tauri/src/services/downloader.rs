use crate::models::DownloadProgress;
use reqwest;
use std::fs;
use std::path::PathBuf;
use std::process::{Command as StdCommand, Stdio};
use std::sync::{Arc, Mutex};
use tokio::process::Command;
use tokio::sync::broadcast;
use url::Url;
use futures::stream::{self, StreamExt};

/// 正在下载的视频ID集合
pub static DOWNLOADING_VIDEOS: std::sync::LazyLock<Arc<Mutex<Vec<String>>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(Vec::new())));

/// 检查视频是否正在下载
pub fn is_downloading(video_id: &str) -> bool {
    let downloading = DOWNLOADING_VIDEOS.lock().unwrap();
    downloading.contains(&video_id.to_string())
}

/// 标记视频开始下载
pub fn start_download(video_id: &str) {
    let mut downloading = DOWNLOADING_VIDEOS.lock().unwrap();
    if !downloading.contains(&video_id.to_string()) {
        downloading.push(video_id.to_string());
    }
}

/// 标记视频下载完成
pub fn finish_download(video_id: &str) {
    let mut downloading = DOWNLOADING_VIDEOS.lock().unwrap();
    downloading.retain(|id| id != video_id);
}

/// 获取项目目录中的ffmpeg路径（用于dev模式）
fn get_project_ffmpeg_path() -> Option<String> {
    // 从当前exe或crate根目录查找项目ffmpeg目录
    let exe_path = std::env::current_exe().ok()?;
    let crate_root = exe_path
        .parent()?
        .parent()?
        .parent()?
        .join("src-tauri")
        .join("ffmpeg")
        .join("ffmpeg");

    if crate_root.exists() {
        return Some(crate_root.to_string_lossy().to_string());
    }

    // 也尝试从当前工作目录查找
    let current_dir = std::env::current_dir().ok()?;
    let project_ffmpeg = current_dir.join("src-tauri").join("ffmpeg").join("ffmpeg");
    if project_ffmpeg.exists() {
        return Some(project_ffmpeg.to_string_lossy().to_string());
    }

    None
}

/// 获取打包的ffmpeg路径（用于生产模式）
fn get_bundled_ffmpeg_path() -> Option<String> {
    // Tauri 打包的资源路径: {exe}/../resources/ffmpeg/ffmpeg
    let resource_path = std::env::current_exe()
        .ok()?
        .parent()?
        .parent()?
        .join("resources")
        .join("ffmpeg");

    let ffmpeg_path = if cfg!(target_os = "macos") {
        resource_path.join("ffmpeg")
    } else if cfg!(target_os = "windows") {
        resource_path.join("ffmpeg.exe")
    } else {
        resource_path.join("ffmpeg")
    };

    if ffmpeg_path.exists() {
        Some(ffmpeg_path.to_string_lossy().to_string())
    } else {
        None
    }
}

/// 检查ffmpeg是否可用
pub fn check_ffmpeg() -> bool {
    // 先检查项目目录中的ffmpeg（dev模式）
    if get_project_ffmpeg_path().is_some() {
        return true;
    }

    // 再检查打包的ffmpeg（生产模式）
    if get_bundled_ffmpeg_path().is_some() {
        return true;
    }

    // 检查系统ffmpeg
    let output = StdCommand::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    output.is_ok()
}

/// 获取ffmpeg路径
pub fn find_ffmpeg() -> String {
    // 先检查项目目录中的ffmpeg（dev模式）
    if let Some(path) = get_project_ffmpeg_path() {
        return path;
    }

    // 再检查打包的ffmpeg（生产模式）
    if let Some(path) = get_bundled_ffmpeg_path() {
        return path;
    }

    let possible_paths = vec![
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/usr/bin/ffmpeg",
    ];

    for path in possible_paths {
        if PathBuf::from(path).exists() {
            return path.to_string();
        }
    }

    // 尝试which命令
    if let Ok(output) = StdCommand::new("which").arg("ffmpeg").output() {
        if output.status.success() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                return s.trim().to_string();
            }
        }
    }

    "ffmpeg".to_string()
}

/// 解析m3u8加密信息
struct EncryptionInfo {
    encrypted: bool,
    key_url: Option<String>,
    iv: Option<String>,
}

/// 从m3u8内容解析加密信息
fn parse_encryption_info(m3u8_content: &str, m3u8_url: &str) -> EncryptionInfo {
    let mut info = EncryptionInfo {
        encrypted: false,
        key_url: None,
        iv: None,
    };

    for line in m3u8_content.lines() {
        let line = line.trim();
        if line.starts_with("#EXT-X-KEY:") {
            if line.contains("METHOD=AES-128") {
                info.encrypted = true;

                // 提取URI
                if let Some(uri_start) = line.find("URI=\"") {
                    let uri_end = line[uri_start + 5..].find("\"").map(|i| uri_start + 5 + i);
                    if let Some(end) = uri_end {
                        let key_uri = &line[uri_start + 5..end];
                        // 补全key URL
                        let key_url = if key_uri.starts_with("http") {
                            key_uri.to_string()
                        } else if key_uri.starts_with('/') {
                            if let Ok(parsed) = Url::parse(m3u8_url) {
                                let host = parsed.host_str().unwrap_or("d1ibyof3mbdf0n.cloudfront.net");
                                format!("{}://{}{}", parsed.scheme(), host, key_uri)
                            } else {
                                format!("https://d1ibyof3mbdf0n.cloudfront.net{}", key_uri)
                            }
                        } else {
                            let base = m3u8_url.rsplit('/').nth(1).map(|s| format!("{}/", s)).unwrap_or_default();
                            format!("{}{}", base, key_uri)
                        };
                        info.key_url = Some(key_url);
                    }
                }

                // 提取IV
                if let Some(iv_start) = line.find("IV=0x") {
                    let iv_str = &line[iv_start + 5..];
                    info.iv = Some(iv_str.to_string());
                }
            }
        }
    }

    info
}

/// 下载M3U8视频（支持AES-128加密）
pub async fn download_m3u8(
    m3u8_url: &str,
    output_path: &str,
    video_id: &str,
    video_name: &str,
    mut progress_callback: impl FnMut(DownloadProgress),
) -> Result<(), String> {
    // 检查ffmpeg
    if !check_ffmpeg() {
        return Err("未找到ffmpeg，请先安装: brew install ffmpeg".to_string());
    }

    let ffmpeg_path = find_ffmpeg();
    let output_dir = PathBuf::from(output_path);
    let _ = fs::create_dir_all(&output_dir);

    // 生成临时文件名（只保留ASCII字符，避免路径问题）
    let temp_filename = video_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect::<String>();
    // 最终输出文件也用临时文件名，避免中文路径问题
    let video_path = output_dir.join(format!("{}.mp4", temp_filename));
    let temp_dir = output_dir.join(format!("{}_temp", temp_filename));
    let playlist_path = temp_dir.join("playlist.m3u8");
    let key_path = temp_dir.join("decrypt.key");
    let _ = fs::create_dir_all(&temp_dir);

    // eprintln!("[DOWNLOAD] temp_filename: {}", temp_filename);
    // eprintln!("[DOWNLOAD] final video_path: {:?}", video_path);
    // eprintln!("[DOWNLOAD] temp_dir: {:?}", temp_dir);

    // 下载m3u8文件
    progress_callback(DownloadProgress {
        video_id: video_id.to_string(),
        progress: 5,
        status: "正在下载m3u8文件...".to_string(),
        speed: "0 MB/s".to_string(),
        eta: "--:--".to_string(),
    });

    let client = reqwest::Client::new();
    let m3u8_content = if let Ok(response) = client.get(m3u8_url).send().await {
        if response.status().is_success() {
            if let Ok(content) = response.text().await {
                let _ = fs::write(&playlist_path, &content);
                content
            } else {
                return Err("无法读取m3u8响应内容".to_string());
            }
        } else {
            return Err(format!("m3u8下载失败，HTTP状态码: {}", response.status()));
        }
    } else {
        return Err("无法连接到m3u8服务器".to_string());
    };

    // 检查m3u8内容是否有效
    if !m3u8_content.contains("#EXTM3U") {
        eprintln!("[DOWNLOAD] m3u8 content (first 500 chars): {}", &m3u8_content[..500.min(m3u8_content.len())]);
        return Err("m3u8文件内容无效或不包含#EXTM3U".to_string());
    }

    // eprintln!("[DOWNLOAD] m3u8 content length: {}", m3u8_content.len());
    // eprintln!("[DOWNLOAD] m3u8 first 300 chars: {}", &m3u8_content[..300.min(m3u8_content.len())]);

    progress_callback(DownloadProgress {
        video_id: video_id.to_string(),
        progress: 15,
        status: "正在解析播放列表...".to_string(),
        speed: "0 MB/s".to_string(),
        eta: "--:--".to_string(),
    });

    // 检查加密状态
    let encryption_info = parse_encryption_info(&m3u8_content, m3u8_url);
    // eprintln!("[DOWNLOAD] encrypted: {:?}", encryption_info.encrypted);
    // eprintln!("[DOWNLOAD] key_url: {:?}", encryption_info.key_url);

    let mut local_key_path: Option<String> = None;

    if encryption_info.encrypted {
        progress_callback(DownloadProgress {
            video_id: video_id.to_string(),
            progress: 20,
            status: "检测到AES-128加密，正在获取密钥...".to_string(),
            speed: "0 MB/s".to_string(),
            eta: "--:--".to_string(),
        });

        if let Some(key_url) = encryption_info.key_url {
            // 从m3u8_url提取token参数
            let m3u8_parsed = Url::parse(m3u8_url).ok();
            let query = m3u8_parsed.as_ref().and_then(|u| u.query()).unwrap_or("");

            // 构造带token的key请求URL
            let key_url_with_token = if query.is_empty() {
                key_url.clone()
            } else if key_url.contains('?') {
                format!("{}&{}", key_url, query)
            } else {
                format!("{}?{}", key_url, query)
            };

            // eprintln!("[DOWNLOAD] key_url_with_token: {}", key_url_with_token);

            // 下载密钥
            match client.get(&key_url_with_token).send().await {
                Ok(resp) => {
                    // eprintln!("[DOWNLOAD] key response status: {}", resp.status());
                    if resp.status().is_success() {
                        match resp.bytes().await {
                            Ok(key_data) => {
                                // eprintln!("[DOWNLOAD] key_data length: {}", key_data.len());
                                let _ = fs::write(&key_path, &key_data);
                                local_key_path = Some(key_path.to_string_lossy().to_string());
                                // eprintln!("[DOWNLOAD] key saved to: {:?}", key_path);

                                progress_callback(DownloadProgress {
                                    video_id: video_id.to_string(),
                                    progress: 25,
                                    status: "密钥获取成功".to_string(),
                                    speed: "0 MB/s".to_string(),
                                    eta: "--:--".to_string(),
                                });
                            }
                            Err(e) => {
                                return Err(format!("读取密钥响应失败: {}", e));
                            }
                        }
                    } else {
                        return Err(format!("获取密钥失败，HTTP状态码: {}", resp.status()));
                    }
                }
                Err(e) => {
                    return Err(format!("请求密钥失败: {}", e));
                }
            }
        }
    }

    // 读取并修改m3u8文件中的相对路径和密钥URI
    progress_callback(DownloadProgress {
        video_id: video_id.to_string(),
        progress: 30,
        status: "正在处理播放列表...".to_string(),
        speed: "0 MB/s".to_string(),
        eta: "--:--".to_string(),
    });

    if let Ok(content) = fs::read_to_string(&playlist_path) {
        let base_url = m3u8_url.rsplit('/').nth(1).map(|s| format!("{}/", s)).unwrap_or_default();
        let modified: String = content
            .lines()
            .map(|line| {
                let line = line.trim();
                // 处理密钥URI替换 - 支持带引号和不带引号的格式
                if line.starts_with("#EXT-X-KEY:METHOD=AES-128,URI=") {
                    if let Some(local_key) = &local_key_path {
                        // eprintln!("[DOWNLOAD] Replacing key URI with: {}", local_key);
                        return format!("#EXT-X-KEY:METHOD=AES-128,URI=\"{}\"", local_key);
                    }
                }
                // 处理相对路径
                if line.starts_with("#") || line.is_empty() {
                    line.to_string()
                } else if !line.starts_with("http") {
                    if line.starts_with('/') {
                        format!("https://d1ibyof3mbdf0n.cloudfront.net{}", line)
                    } else {
                        format!("{}{}", base_url, line)
                    }
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let _ = fs::write(&playlist_path, &modified);
        // eprintln!("[DOWNLOAD] Modified m3u8 saved");
        // eprintln!("[DOWNLOAD] Modified content preview (first 500 chars):\n{}", &modified[..500.min(modified.len())]);
    } else {
        eprintln!("[DOWNLOAD] ERROR: Could not read playlist file");
    }

    progress_callback(DownloadProgress {
        video_id: video_id.to_string(),
        progress: 40,
        status: "正在调用ffmpeg下载视频...".to_string(),
        speed: "0 MB/s".to_string(),
        eta: "--:--".to_string(),
    });

    // 使用 tokio::process::Command 异步执行 ffmpeg
    let output = Command::new(&ffmpeg_path)
        .args(&[
            "-y",
            "-protocol_whitelist", "file,http,https,tcp,tls,crypto",
            "-allowed_extensions", "ALL",
            "-i", playlist_path.to_str().unwrap_or("playlist.m3u8"),
            "-c", "copy",
            "-bsf:a", "aac_adtstoasc",
            video_path.to_str().unwrap_or("output.mp4"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    match output {
        Ok(result) => {
            if result.status.success() {
                // 清理临时文件
                let _ = fs::remove_dir_all(&temp_dir);

                // 如果原视频名包含中文，重命名为中文名
                let final_path = if video_name != temp_filename {
                    let target_path = output_dir.join(format!("{}.mp4", video_name));
                    if let Err(e) = fs::rename(&video_path, &target_path) {
                        eprintln!("[DOWNLOAD] 重命名失败，使用临时文件名: {}", e);
                    } else {
                        // eprintln!("[DOWNLOAD] 已重命名为: {}", video_name);
                    }
                    target_path
                } else {
                    video_path
                };

                // 检查最终文件是否存在
                if final_path.exists() {
                    progress_callback(DownloadProgress {
                        video_id: video_id.to_string(),
                        progress: 100,
                        status: "下载完成".to_string(),
                        speed: "0 MB/s".to_string(),
                        eta: "00:00".to_string(),
                    });
                    return Ok(());
                }

                progress_callback(DownloadProgress {
                    video_id: video_id.to_string(),
                    progress: 100,
                    status: "下载完成".to_string(),
                    speed: "0 MB/s".to_string(),
                    eta: "00:00".to_string(),
                });

                Ok(())
            } else {
                let error_msg = String::from_utf8_lossy(&result.stderr);
                // 尝试不使用-c copy重新编码
                progress_callback(DownloadProgress {
                    video_id: video_id.to_string(),
                    progress: 40,
                    status: "ffmpeg错误，尝试重新编码...".to_string(),
                    speed: "0 MB/s".to_string(),
                    eta: "--:--".to_string(),
                });

                let retry_output = Command::new(&ffmpeg_path)
                    .args(&[
                        "-y",
                        "-protocol_whitelist", "file,http,https,tcp,tls,crypto",
                        "-allowed_extensions", "ALL",
                        "-i", playlist_path.to_str().unwrap_or("playlist.m3u8"),
                        "-c:v", "libx264",
                        "-c:a", "aac",
                        video_path.to_str().unwrap_or("output.mp4"),
                    ])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .await;

                if retry_output.map(|o| o.status.success()).unwrap_or(false) {
                    let _ = fs::remove_dir_all(&temp_dir);

                    // 如果原视频名包含中文，重命名为中文名
                    if video_name != temp_filename {
                        let target_path = output_dir.join(format!("{}.mp4", video_name));
                        if let Err(e) = fs::rename(&video_path, &target_path) {
                            eprintln!("[DOWNLOAD] 重命名失败，使用临时文件名: {}", e);
                        } else {
                            // eprintln!("[DOWNLOAD] 已重命名为: {}", video_name);
                        }
                    }

                    progress_callback(DownloadProgress {
                        video_id: video_id.to_string(),
                        progress: 100,
                        status: "下载完成".to_string(),
                        speed: "0 MB/s".to_string(),
                        eta: "00:00".to_string(),
                    });
                    return Ok(());
                }

                Err(format!("ffmpeg错误: {}", error_msg))
            }
        }
        Err(e) => Err(format!("执行ffmpeg失败: {}", e)),
    }
}

/// 并发批量下载视频
pub async fn batch_download_concurrent(
    videos: Vec<(String, String, String, PathBuf)>,
    max_concurrent: usize,
    progress_sender: broadcast::Sender<DownloadProgress>,
) -> Vec<(String, Result<(), String>)> {
    // 使用 tokio::stream 并发执行下载
    let results = stream::iter(videos.into_iter().map(|(id, name, m3u8_url, output_dir)| {
        let sender = progress_sender.clone();
        async move {
            let video_id = id.clone();
            let name_clone = name.clone();
            let sender_for_callback = sender.clone();

            // 标记开始下载
            start_download(&video_id);

            // 发送开始下载消息
            let _ = sender.send(DownloadProgress {
                video_id: video_id.clone(),
                progress: 0,
                status: "准备下载...".to_string(),
                speed: "0 MB/s".to_string(),
                eta: "--:--".to_string(),
            });

            // 定义进度回调
            let progress_callback = move |p: DownloadProgress| {
                let _ = sender_for_callback.send(p);
            };

            // 执行下载
            let result = download_m3u8(&m3u8_url, &output_dir.to_string_lossy(), &video_id, &name, progress_callback).await;

            // 标记下载完成
            finish_download(&video_id);

            // 发送完成消息
            if result.is_ok() {
                let _ = sender.send(DownloadProgress {
                    video_id: video_id.clone(),
                    progress: 100,
                    status: "下载完成".to_string(),
                    speed: "0 MB/s".to_string(),
                    eta: "00:00".to_string(),
                });
            } else if let Err(ref err) = result {
                let _ = sender.send(DownloadProgress {
                    video_id: video_id.clone(),
                    progress: 0,
                    status: format!("下载失败: {}", err),
                    speed: "0 MB/s".to_string(),
                    eta: "--:--".to_string(),
                });
            }

            (name_clone, result)
        }
    }))
    .buffer_unordered(max_concurrent)
    .collect()
    .await;

    results
}
