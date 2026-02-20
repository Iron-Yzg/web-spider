//! HLS HTTP 服务器 - 为解复用后的视频提供 HTTP 流服务
//!
//! 监听本地端口，将 HLS 文件通过 HTTP 协议提供给前端

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::Mutex;
use hyper::{Body, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};

/// HLS 服务器状态
pub struct HlsServer {
    port: u16,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl HlsServer {
    /// 启动 HLS 服务器
    pub async fn start(base_path: PathBuf) -> Result<Self, String> {
        // 查找可用端口
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .map_err(|e| format!("无法绑定端口: {}", e))?;

        let port = listener.local_addr()
            .map_err(|e| format!("无法获取端口: {}", e))?
            .port();

        tracing::info!("[hls-server] 启动在端口: {}", port);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let base_path = Arc::new(base_path);

        // 启动 HTTP 服务
        tokio::spawn(async move {
            let base_path = base_path.clone();

            let make_svc = make_service_fn(move |_conn| {
                let base_path = base_path.clone();
                async move {
                    Ok::<_, Infallible>(service_fn(move |req: Request<Body>| {
                        let base_path = base_path.clone();
                        async move {
                            handle_request(req, base_path).await
                        }
                    }))
                }
            });

            let server = Server::from_tcp(listener.into_std().unwrap())
                .unwrap()
                .serve(make_svc);

            // 监听关闭信号
            let graceful = server.with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
                tracing::info!("[hls-server] 收到关闭信号");
            });

            if let Err(e) = graceful.await {
                tracing::error!("[hls-server] 服务器错误: {}", e);
            }

            tracing::info!("[hls-server] 服务器已停止");
        });

        Ok(Self {
            port,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// 获取服务器 URL
    pub fn get_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// 停止服务器
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

/// 处理 HTTP 请求
async fn handle_request(req: Request<Body>, base_path: Arc<PathBuf>) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path();
    let file_path = base_path.join(path.trim_start_matches('/'));

    tracing::debug!("[hls-server] 请求: {} -> {:?}", path, file_path);

    // 安全检查：确保文件在 base_path 内
    let canonical_base = match tokio::fs::canonicalize(&*base_path).await {
        Ok(p) => p,
        Err(_) => {
            return Ok(Response::builder()
                .status(500)
                .body(Body::from("Internal Server Error"))
                .unwrap());
        }
    };

    let canonical_file = match tokio::fs::canonicalize(&file_path).await {
        Ok(p) => p,
        Err(_) => {
            return Ok(Response::builder()
                .status(404)
                .body(Body::from("Not Found"))
                .unwrap());
        }
    };

    if !canonical_file.starts_with(&canonical_base) {
        return Ok(Response::builder()
            .status(403)
            .body(Body::from("Forbidden"))
            .unwrap());
    }

    // 读取文件
    match tokio::fs::read(&file_path).await {
        Ok(content) => {
            let content_type = if path.ends_with(".m3u8") {
                "application/vnd.apple.mpegurl"
            } else if path.ends_with(".ts") {
                "video/mp2t"
            } else {
                "application/octet-stream"
            };

            Ok(Response::builder()
                .status(200)
                .header("Content-Type", content_type)
                .header("Access-Control-Allow-Origin", "*")
                .header("Cache-Control", "no-cache")
                .body(Body::from(content))
                .unwrap())
        }
        Err(_) => {
            Ok(Response::builder()
                .status(404)
                .body(Body::from("Not Found"))
                .unwrap())
        }
    }
}

/// 运行中的 HLS 服务器
static HLS_SERVERS: std::sync::LazyLock<Mutex<HashMap<String, HlsServer>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// 启动 HLS 服务器并返回播放 URL
pub async fn start_hls_server(session_id: String, hls_dir: PathBuf) -> Result<String, String> {
    // 停止已有的服务器
    stop_hls_server(&session_id).await.ok();

    // 启动新服务器
    let server = HlsServer::start(hls_dir).await?;
    let url = format!("{}/playlist.m3u8", server.get_url());
    
    // 保存服务器实例
    {
        let mut servers = HLS_SERVERS.lock().await;
        servers.insert(session_id.clone(), server);
    }
    
    tracing::info!("[hls-server] 会话 {} 的播放地址: {}", session_id, url);
    Ok(url)
}

/// 停止 HLS 服务器
pub async fn stop_hls_server(session_id: &str) -> Result<(), String> {
    let mut servers = HLS_SERVERS.lock().await;
    if let Some(mut server) = servers.remove(session_id) {
        server.stop();
        tracing::info!("[hls-server] 已停止会话 {}", session_id);
    }
    Ok(())
}

/// 清理所有 HLS 服务器
pub async fn cleanup_all_hls_servers() {
    let mut servers = HLS_SERVERS.lock().await;
    for (session_id, mut server) in servers.drain() {
        server.stop();
        tracing::info!("[hls-server] 清理会话 {}", session_id);
    }
}
