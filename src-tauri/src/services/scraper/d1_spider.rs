use crate::models::{LocalStorageItem, ScrapeResult, Website};
use crate::services::scraper::Scraper;
use headless_chrome::Browser;
use std::ffi::OsStr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::future::Future;

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

        Box::pin(async move {
            let page_url = format!("{}subPage/longViodePlay/?id={}", base_url, video_id);

            // 创建日志函数
            let log = {
                let callback = log_callback.clone();
                move |msg: String| {
                    eprintln!("[SCRAPER] {}", msg);
                    callback(msg);
                }
            };

            log(format!("正在爬取: {}", video_id));

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
                    headless: true,
                    ..Default::default()
                }
            ) {
                Ok(browser) => browser,
                Err(e) => {
                    log(format!("启动浏览器失败: {}", e));
                    return ScrapeResult {
                        success: false,
                        name: String::new(),
                        m3u8_url: String::new(),
                        message: format!("启动浏览器失败: {}", e),
                    };
                }
            };

            let tab = match browser.new_tab() {
                Ok(tab) => tab,
                Err(e) => {
                    log(format!("创建标签页失败: {}", e));
                    return ScrapeResult {
                        success: false,
                        name: String::new(),
                        m3u8_url: String::new(),
                        message: format!("创建标签页失败: {}", e),
                    };
                }
            };

            // 创建共享的 m3u8 URL 捕获变量
            let captured_url = Arc::new(Mutex::new(None::<String>));

            // 注册网络响应处理器
            let captured_url_clone = Arc::clone(&captured_url);
            let log_callback_for_response = Arc::new(log_callback);

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
                            eprintln!("[SCRAPER] {}", msg);
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
                log(format!("导航到页面 (第{}次尝试)...", attempt));
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
                log(format!("导航失败: {}", nav_error));
                let _ = tab.close(true);
                drop(tab);
                drop(browser);
                return ScrapeResult {
                    success: false,
                    name: String::new(),
                    m3u8_url: String::new(),
                    message: format!("导航失败: {}", nav_error),
                };
            }

            // 如果有 localStorage，注入
            if !local_storage.is_empty() {
                log("注入 localStorage...".to_string());
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
                log("刷新页面...".to_string());
                let _ = tab.reload(true, None);
                // 等待页面重新加载完成
                tokio::time::sleep(Duration::from_secs(2)).await;
            }

            // 等待 m3u8 请求，最多等待 15 秒
            let mut found_url = None;
            let start_time = std::time::Instant::now();
            let timeout = Duration::from_secs(15);

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
                        log("资源不存在".to_string());
                        let _ = tab.close(true);
                        drop(tab);
                        drop(browser);
                        return ScrapeResult {
                            success: false,
                            name: String::new(),
                            m3u8_url: String::new(),
                            message: "资源不存在，该视频可能已被删除或ID无效".to_string(),
                        };
                    }
                }

                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            // 如果通过网络拦截没有找到，尝试从 HTML 中查找
            if found_url.is_none() {
                log("网络拦截未找到，尝试从 HTML 查找...".to_string());

                let page_html: String = match tab.get_content() {
                    Ok(html) => html,
                    Err(_) => String::new(),
                };

                let m3u8_pattern = regex::Regex::new(r#"https://[^"'\s<>]*\.m3u8[^"'\s<>"']*"#).unwrap();
                let m3u8_urls: Vec<String> = m3u8_pattern
                    .find_iter(&page_html)
                    .map(|m| m.as_str().to_string())
                    .filter(|url| url.contains("/api/app/media/h5/m3u8/"))
                    .collect();

                if let Some(m3u8_url) = m3u8_urls.last() {
                    found_url = Some(m3u8_url.clone());
                    log(format!("从 HTML 找到 m3u8: {}", m3u8_url));
                }
            }

            // 如果找到 m3u8，立即提取标题
            if let Some(ref m3u8_url) = found_url {
                log("正在提取视频标题...".to_string());

                // 提取视频名称 - 使用 Element API
                let name = if let Ok(element) = tab.wait_for_xpath("//div[@class='video-title']") {
                    log("元素 div.video-title 已找到".to_string());
                    match element.get_inner_text() {
                        Ok(text) => {
                            let trimmed = text.trim().to_string();
                            if trimmed.is_empty() {
                                log("元素内容为空".to_string());
                                format!("视频_{}", video_id)
                            } else {
                                log(format!("从元素提取到文本: {}", trimmed));
                                trimmed
                            }
                        }
                        Err(e) => {
                            log(format!("get_inner_text 失败: {:?}", e));
                            format!("视频_{}", video_id)
                        }
                    }
                } else {
                    log("未找到 video-title 元素".to_string());
                    format!("视频_{}", video_id)
                };

                if name == format!("视频_{}", video_id) {
                    log(format!("DOM 提取为空，使用默认名称: {}", name));
                } else {
                    log(format!("从 DOM 提取到视频名称: {}", name));
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

                log(format!("完成: {}", final_url));

                ScrapeResult {
                    success: true,
                    name: name,
                    m3u8_url: final_url,
                    message: "成功找到 m3u8 地址".to_string(),
                }
            } else {
                // 未找到 m3u8
                let _ = tab.close(true);
                drop(tab);
                drop(browser);

                log("未找到 m3u8 地址".to_string());
                ScrapeResult {
                    success: false,
                    name: String::new(),
                    m3u8_url: String::new(),
                    message: "未能找到 m3u8 地址".to_string(),
                }
            }
        })
    }
}
