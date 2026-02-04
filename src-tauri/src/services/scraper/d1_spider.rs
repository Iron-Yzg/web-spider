use crate::models::{LocalStorageItem, ScrapeResult, Website};
use crate::services::scraper::Scraper;
use headless_chrome::Browser;
use std::ffi::OsStr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::future::Future;
use url::Url;

/// D1 Cloudfront 爬虫 - 专门爬取 d1ibyof3mbdf0n.cloudfront.net
#[derive(Clone)]
pub struct D1Spider {
    base_url: String,
    local_storage: Vec<LocalStorageItem>,
}

impl D1Spider {
    pub fn new(website: &Website) -> Self {
        Self {
            base_url: website.base_url.clone(),
            local_storage: website.local_storage.clone(),
        }
    }

    #[allow(dead_code)]
    /// 从 localStorage 中获取 token 值
    pub fn get_token_from_local_storage(&self) -> Option<String> {
        self.local_storage
            .iter()
            .find(|item| item.key == "token")
            .map(|item| item.value.clone())
    }

    #[allow(dead_code)]
    /// 从 URL 中提取 token 参数
    pub fn extract_url_token(url: &str) -> Option<String> {
        if let Ok(parsed) = Url::parse(url) {
            if let Some(query) = parsed.query() {
                for param in query.split('&') {
                    if param.starts_with("token=") {
                        return Some(param[6..].to_string());
                    }
                }
            }
        }
        None
    }

    #[allow(dead_code)]
    /// 更新 URL 中的 token
    /// 如果 URL 中的 token 与新 token 不同，则替换
    pub fn update_url_token(url: &str, new_token: &str) -> String {
        if let Ok(parsed) = Url::parse(url) {
            if let Some(query) = parsed.query() {
                // 检查 URL 中是否已有 token 参数
                let params: Vec<&str> = query.split('&').collect();
                let mut has_token = false;
                let mut new_params: Vec<String> = Vec::new();

                for param in params {
                    if param.starts_with("token=") {
                        has_token = true;
                        new_params.push(format!("token={}", new_token));
                    } else {
                        new_params.push(param.to_string());
                    }
                }

                if has_token {
                    // 重建 URL
                    let mut new_url = parsed;
                    new_url.set_query(Some(&new_params.join("&")));
                    new_url.to_string()
                } else {
                    // 没有 token 参数，直接返回原 URL
                    url.to_string()
                }
            } else {
                // 没有查询参数，返回原 URL
                url.to_string()
            }
        } else {
            url.to_string()
        }
    }
}

/// 解析数字字符串（如 "1.7万"、"991"）为 i64
fn parse_count(count_str: &str) -> Option<i64> {
    let cleaned = count_str.trim()
        .replace(",", "")
        .replace(" ", "");

    // 处理中文数字（万）
    if let Some(idx) = cleaned.find('万') {
        let num_part = &cleaned[..idx];
        if let Ok(num) = num_part.parse::<f64>() {
            return Some((num * 10000.0) as i64);
        }
    }

    cleaned.parse::<i64>().ok()
}

impl Scraper for D1Spider {
    fn id(&self) -> &'static str {
        "d1"
    }

    fn scrape(
        &self,
        video_id: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = ScrapeResult> + Send>> {
        // 克隆需要在 async 块外捕获的值
        let video_id = video_id.to_string();
        let base_url = self.base_url.clone();
        let local_storage = self.local_storage.clone();
        let log_callback = log_callback.clone();

        Box::pin(async move {
            let page_url = format!("{}subPage/longViodePlay/?id={}", base_url, video_id);
            let _ = log_callback(format!("正在爬取: {}", page_url));

            // 使用明确的 headless 模式参数
            let browser_args: Vec<&OsStr> = vec![
                OsStr::new("--headless=new"),
                OsStr::new("--no-sandbox"),
                OsStr::new("--disable-dev-shm-usage"),
                OsStr::new("--disable-gpu"),
                OsStr::new("--disable-software-rasterizer"),
                OsStr::new("--mute-audio"),
                OsStr::new("--hide-scrollbars"),
                OsStr::new("--disable-translate"),
                OsStr::new("--disable-background-networking"),
                OsStr::new("--disable-sync"),
                OsStr::new("--disable-features=site-per-process,TranslateUI"),
                OsStr::new("--disable-extensions"),
            ];

            let browser = match Browser::new(
                headless_chrome::LaunchOptions {
                    args: browser_args,
                    headless: false,
                    ..Default::default()
                }
            ) {
                Ok(browser) => browser,
                Err(e) => {
                    return ScrapeResult {
                        success: false,
                        name: String::new(),
                        m3u8_url: String::new(),
                        message: format!("启动浏览器失败: {}", e),
                        video_id: Some(video_id.clone()),
                        view_count: None,
                        favorite_count: None,
                        cover_url: None,
                    };
                }
            };

            let tab = match browser.new_tab() {
                Ok(tab) => tab,
                Err(e) => {
                    return ScrapeResult {
                        success: false,
                        name: String::new(),
                        m3u8_url: String::new(),
                        message: format!("创建标签页失败: {}", e),
                        video_id: Some(video_id.clone()),
                        view_count: None,
                        favorite_count: None,
                        cover_url: None,
                    };
                }
            };

            // 创建共享的 m3u8 URL 捕获变量
            let captured_url = Arc::new(Mutex::new(None::<String>));

            // 注册网络响应处理器
            let captured_url_clone = Arc::clone(&captured_url);
            let log_callback_for_response = Arc::new(log_callback.clone());

            // 注册网络响应处理器
            let _ = tab.register_response_handling(
                "m3u8_capture",
                Box::new(move |params, _fetch_body| {
                    let url = params.response.url.clone();
                    if url.contains(".m3u8") && url.contains("/api/app/media/h5/m3u8/") {
                        let mut captured = captured_url_clone.lock().unwrap();
                        if captured.is_none() {
                            *captured = Some(url.clone());
                            let msg = format!("捕获到m3u8: {}", url);
                            // tracing::info!("[SCRAPER] {}", msg);
                            log_callback_for_response(msg);
                        }
                    }
                }),
            );

            // 等待浏览器稳定
            tokio::time::sleep(Duration::from_millis(500)).await;

            // 导航到页面（带重试）
            let mut nav_success = false;
            let mut nav_error = String::new();
            for attempt in 1..=3 {
                match tab.navigate_to(&page_url) {
                    Ok(_) => {
                        nav_success = true;
                        break;
                    }
                    Err(e) => {
                        nav_error = format!("{}", e);
                        if attempt < 3 {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            }

            if !nav_success {
                let _ = tab.close(true);
                drop(tab);
                drop(browser);
                return ScrapeResult {
                    success: false,
                    name: String::new(),
                    m3u8_url: String::new(),
                    message: format!("导航失败: {}", nav_error),
                    video_id: Some(video_id.clone()),
                    view_count: None,
                    favorite_count: None,
                    cover_url: None,
                };
            }

            // 如果有 localStorage，注入
            if !local_storage.is_empty() {
                for item in &local_storage {
                    let key = item.key.clone();
                    let value = item.value.clone();
                    let inject_js = format!(
                        r#"localStorage.setItem('{}', '{}');"#,
                        key.replace("'", "\\'"),
                        value.replace("'", "\\'")
                    );
                    let _ = tab.evaluate(&inject_js, false);
                }
                let _ = tab.reload(true, None);
                let _ = log_callback(format!("已注入 {} 个 localStorage 项", local_storage.len()));
                // 等待页面重新加载完成
                tokio::time::sleep(Duration::from_secs(2)).await;
            }

            // 等待 m3u8 请求，最多等待 15 秒
            let mut found_url = None;
            let start_time = std::time::Instant::now();
            let timeout = Duration::from_secs(10);

            while start_time.elapsed() < timeout {
                // 检查是否已捕获到 m3u8
                {
                    let captured = captured_url.lock().unwrap();
                    if let Some(url) = captured.clone() {
                        found_url = Some(url.clone());
                        break;
                    }
                }

                // 定期检查页面是否显示 404
                let body_text: String = match tab.evaluate("document.body.innerText", false) {
                    Ok(result) => result.value.unwrap_or_default().as_str().unwrap_or("").to_string(),
                    Err(_) => String::new(),
                };

                // 修正: 只在真正确认是 404 页面时才返回错误
                // 如果已经捕获到 m3u8，就不检查 404 了
                if found_url.is_none() {
                    if body_text.contains("资源不存在") && body_text.contains("404") {
                        let _ = tab.close(true);
                        drop(tab);
                        drop(browser);
                        return ScrapeResult {
                            success: false,
                            name: String::new(),
                            m3u8_url: String::new(),
                            message: "资源不存在，该视频可能已被删除或ID无效".to_string(),
                            video_id: Some(video_id.clone()),
                            view_count: None,
                            favorite_count: None,
                            cover_url: None,
                        };
                    }
                }

                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            // 如果找到 m3u8，立即提取标题和其他数据
            let mut name = format!("视频_{}", video_id);
            let mut view_count: Option<i64> = None;
            let mut favorite_count: Option<i64> = None;
            let mut cover_url: Option<String> = None;

            if let Some(ref m3u8_url) = found_url {
                let _ = log_callback("正在提取视频信息...".to_string());

                // 提取视频名称
                if let Ok(element) = tab.wait_for_xpath("//div[@class='video-title']") {
                    if let Ok(text) = element.get_inner_text() {
                        let trimmed = text.trim().to_string();
                        if !trimmed.is_empty() {
                            name = trimmed;
                            let _ = log_callback(format!("视频名称: {}", name));
                        }
                    }
                }

                // 提取播放数和收藏数
                // DOM结构: <li data-v-3298636b=""><div data-v-3298636b=""><i class="van-icon van-icon-like"></i> 1.7万</div><div data-v-3298636b=""><i class="van-icon van-icon-star"></i> 991</div></li>
                let count_js = r#"
                    (() => {
                        let playText = '';
                        let favText = '';
                        // 查找包含 van-icon-like 的元素附近的 div
                        const likeIcon = document.querySelector('.van-icon-like');
                        if (likeIcon && likeIcon.parentElement) {
                            playText = likeIcon.parentElement.innerText.trim();
                        }
                        // 查找包含 van-icon-star 的元素
                        const starIcon = document.querySelector('.van-icon-star');
                        if (starIcon && starIcon.parentElement) {
                            favText = starIcon.parentElement.innerText.trim();
                        }
                        // 返回简单字符串格式
                        return playText + '|' + favText;
                    })()
                "#;

                let _ = log_callback("正在提取播放和收藏数...".to_string());
                if let Ok(result) = tab.evaluate(count_js, false) {
                    // tracing::info!("[DEBUG] count_js result: {:?}", result);
                    if let Some(value) = result.value.as_ref().and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                        let parts: Vec<&str> = value.split('|').collect();
                        // tracing::info!("[DEBUG] count_js value: {:?}", value);
                        if parts.len() >= 1 && !parts[0].is_empty() {
                            view_count = parse_count(parts[0]);
                            let _ = log_callback(format!("播放数: {}", parts[0]));
                            // tracing::info!("[DEBUG] play_count: {:?} = {:?}", parts[0], view_count);
                        }
                        if parts.len() >= 2 && !parts[1].is_empty() {
                            favorite_count = parse_count(parts[1]);
                            let _ = log_callback(format!("收藏数: {}", parts[1]));
                            // tracing::info!("[DEBUG] favorite_count: {:?} = {:?}", parts[1], favorite_count);
                        }
                    } else if let Some(ref value) = result.value {
                        let _ = log_callback(format!("[DEBUG] count_js result: {:?}", value));
                    }
                }

                // 捕获视频第一帧作为封面
                let cover_js = r#"
                    (() => {
                        const video = document.querySelector('video');
                        if (video && video.videoWidth > 0) {
                            const canvas = document.createElement('canvas');
                            canvas.width = video.videoWidth;
                            canvas.height = video.videoHeight;
                            const ctx = canvas.getContext('2d');
                            ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
                            return canvas.toDataURL('image/jpeg', 0.8);
                        }
                        return '';
                    })()
                "#;

                // 等待视频加载
                let _ = log_callback("等待视频加载 (3秒)...".to_string());
                tokio::time::sleep(Duration::from_secs(3)).await;

                if let Ok(result) = tab.evaluate(cover_js, false) {
                    if let Some(base64) = result.value.as_ref().and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                        cover_url = Some(base64.to_string());
                        let _ = log_callback(format!("封面截图成功, 长度: {} chars", base64.len()));
                    } else {
                        let _ = log_callback(format!("封面截图失败或为空"));
                    }
                }

                // 清理 _0001
                let mut final_url = m3u8_url.clone();
                if final_url.contains("_0001") {
                    final_url = final_url.replace("_0001", "");
                }

                // 关闭浏览器
                let _ = tab.close(true);
                drop(tab);
                drop(browser);

                ScrapeResult {
                    success: true,
                    name: name,
                    m3u8_url: final_url,
                    message: "成功找到 m3u8 地址".to_string(),
                    video_id: Some(video_id.clone()),
                    view_count,
                    favorite_count,
                    cover_url,
                }
            } else {
                // 未找到 m3u8
                let _ = tab.close(true);
                drop(tab);
                drop(browser);

                ScrapeResult {
                    success: false,
                    name: String::new(),
                    m3u8_url: String::new(),
                    message: "未能找到 m3u8 地址".to_string(),
                    video_id: Some(video_id.clone()),
                    view_count: None,
                    favorite_count: None,
                    cover_url: None,
                }
            }
        })
    }

    /// D1 爬虫只爬取单个视频，scrape_all 返回单个结果
    fn scrape_all(
        &self,
        video_id: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = Vec<ScrapeResult>> + Send + 'static>>
    where
        Self: Sized,
    {
        let result = self.scrape(video_id, log_callback);
        Box::pin(async move {
            vec![result.await]
        })
    }
}
