use crate::models::ScrapeResult;
use crate::services::scraper::Scraper;
use crate::Website;
use headless_chrome::Browser;
use reqwest::Client;
use regex::Regex;
use serde::Deserialize;
use std::ffi::OsStr;
use std::pin::Pin;
use std::time::Duration;
use std::future::Future;
use tokio::sync::mpsc;

/// 视频列表项（包含ID和封面）
#[derive(Debug, Clone, Deserialize)]
pub struct VideoListItem {
    #[serde(rename = "id")]
    pub video_id: String,
    pub cover: Option<String>,
}

/// SRL爬虫 - 使用headless_chrome提取列表，reqwest爬取详情
#[derive(Clone)]
pub struct SrlSpider {
    website: Website,
    client: Client,
}

impl SrlSpider {
    pub fn new(website: &Website) -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            website: website.clone(),
            client,
        }
    }

    /// 获取基础URL（去除末尾斜杠）
    fn base_url(&self) -> String {
        self.website.base_url.trim_end_matches('/').to_string()
    }

    /// 构建完整URL
    fn build_url(&self, path: &str) -> String {
        let base = self.base_url();
        if path.starts_with("http") {
            path.to_string()
        } else if path.starts_with("//") {
            format!("https:{}", path)
        } else if path.starts_with("/") {
            format!("{}{}", base, path)
        } else {
            format!("{}/{}", base, path)
        }
    }

    /// 使用headless_chrome提取视频列表（包含ID和封面），返回日志和视频列表
    async fn extract_video_list_with_chrome(&self, page_number: &str) -> (Vec<String>, Vec<VideoListItem>) {
        let page_url = self.build_url(&format!("/page/{}", page_number));
        let mut logs = vec![format!("[Chrome] 访问列表页: {}", page_url)];

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
                logs.push(format!("[Chrome] 启动浏览器失败: {}", e));
                return (logs, Vec::new());
            }
        };

        let tab = match browser.new_tab() {
            Ok(tab) => tab,
            Err(e) => {
                logs.push(format!("[Chrome] 创建标签页失败: {}", e));
                return (logs, Vec::new());
            }
        };

        // 导航到列表页
        if tab.navigate_to(&page_url).is_err() {
            logs.push("[Chrome] 导航失败".to_string());
            let _ = tab.close(true);
            return (logs, Vec::new());
        }

        // 等待页面加载和JavaScript执行
        logs.push("[Chrome] 等待页面加载...".to_string());
        tokio::time::sleep(Duration::from_secs(2)).await;

        // 提取视频列表
        let extract_js = r#"
            (() => {
                const items = [];
                const cards = document.querySelectorAll('article');
                cards.forEach(card => {
                    const link = card.querySelector('a[href^="/archives/"]');
                    if (link) {
                        const href = link.getAttribute('href');
                        const idMatch = href.match(/archives\/(\d+)\.html/);
                        if (idMatch) {
                            const videoId = idMatch[1];
                            const bgDiv = card.querySelector('.blog-background');
                            let cover = null;
                            if (bgDiv) {
                                const style = bgDiv.getAttribute('style') || '';
                                const match = style.match(/background-image\s*:\s*url\(['"]?([^'"()]+)['"]?\)/);
                                if (match && match[1]) {
                                    cover = match[1];
                                }
                            }
                            items.push({id: videoId, cover: cover});
                        }
                    }
                });
                return JSON.stringify(items);
            })()
        "#;

        let video_items_str = match tab.evaluate(extract_js, false) {
            Ok(result) => result.value.as_ref().and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_else(|| "[]".to_string()),
            Err(_) => "[]".to_string(),
        };

        // 关闭浏览器
        let _ = tab.close(true);

        // 解析视频列表
        let video_items: Vec<VideoListItem> = if video_items_str.starts_with("[") {
            let parsed: Result<Vec<VideoListItem>, _> = serde_json::from_str(&video_items_str);
            match parsed {
                Ok(items) => items,
                Err(e) => {
                    logs.push(format!("[Chrome] 解析失败: {}", e));
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        logs.push(format!("[Chrome] 找到 {} 个视频", video_items.len()));
        (logs, video_items)
    }

    /// 使用reqwest从详情页提取m3u8
    async fn fetch_m3u8_from_detail(&self, video_id: &str) -> Option<String> {
        let video_url = self.build_url(&format!("/archives/{}.html", video_id));

        match self.client.get(&video_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let html = resp.text().await.unwrap_or_default();
                // 提取m3u8 URL
                let m3u8_pattern = Regex::new(r#""([^"]+\.m3u8[^"]*)""#).unwrap();
                if let Some(cap) = m3u8_pattern.captures(&html) {
                    if let Some(m) = cap.get(1) {
                        let url = m.as_str().to_string();
                        return Some(if !url.starts_with("http") {
                            self.build_url(&url)
                        } else {
                            url
                        });
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// 提取标题
    fn extract_title(&self, html: &str) -> String {
        let title_pattern = Regex::new(r#"<h1\s+class="post-title\s*"[^>]*>(.*?)</h1>"#).unwrap();
        if let Some(cap) = title_pattern.captures(html) {
            if let Some(title_cap) = cap.get(1) {
                let clean_pattern = Regex::new(r#"<[^>]+>"#).unwrap();
                let cleaned = clean_pattern.replace_all(title_cap.as_str(), " ");
                return cleaned.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string();
            }
        }
        String::new()
    }
}

impl Scraper for SrlSpider {
    fn id(&self) -> &'static str {
        "srl"
    }

    fn scrape(
        &self,
        page_number: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = ScrapeResult> + Send>> {
        let page_number = page_number.to_string();
        let log_callback = log_callback.clone();
        let spider = self.clone();

        Box::pin(async move {
            // 1. 用headless_chrome提取视频列表（ID+封面）
            let (chrome_logs, video_items) = spider.extract_video_list_with_chrome(&page_number).await;
            for log in chrome_logs {
                log_callback(log);
            }

            if video_items.is_empty() {
                return ScrapeResult {
                    success: false,
                    name: format!("第{}页", page_number),
                    m3u8_url: String::new(),
                    message: "未找到视频链接".to_string(),
                    video_id: None,
                    view_count: None,
                    favorite_count: None,
                    cover_url: None,
                };
            }

            // 2. 并发爬取每个视频的m3u8
            let (result_tx, mut result_rx) = mpsc::channel::<(String, Option<String>)>(video_items.len());

            for item in &video_items {
                let video_id = item.video_id.clone();
                let spider = spider.clone();
                let result_tx = result_tx.clone();
                let log_tx = log_callback.clone();

                tokio::spawn(async move {
                    let m3u8_url = spider.fetch_m3u8_from_detail(&video_id).await;
                    let _ = result_tx.send((video_id.clone(), m3u8_url.clone())).await;
                    // 实时输出日志
                    if m3u8_url.is_some() {
                        log_tx(format!("✓ {}", video_id));
                    } else {
                        log_tx(format!("✗ {}", video_id));
                    }
                });
            }

            let mut results: Vec<(String, Option<String>)> = Vec::new();
            for _ in 0..video_items.len() {
                if let Some((id, m3u8)) = result_rx.recv().await {
                    results.push((id, m3u8));
                }
            }

            // 3. 找到第一个成功的
            for item in &video_items {
                if let Some((_, m3u8_url)) = results.iter().find(|(id, _)| id == &item.video_id) {
                    if let Some(ref url) = m3u8_url {
                        // 获取标题
                        let video_url = spider.build_url(&format!("/archives/{}.html", item.video_id));
                        let html = match spider.client.get(&video_url).send().await {
                            Ok(resp) => match resp.text().await {
                                Ok(text) => text,
                                Err(_) => String::new(),
                            },
                            Err(_) => String::new(),
                        };
                        let title = spider.extract_title(&html);

                        let cover_url = item.cover.clone().filter(|c| c.starts_with("data:image"));

                        return ScrapeResult {
                            success: true,
                            name: if title.is_empty() { format!("视频_{}", item.video_id) } else { title },
                            m3u8_url: url.clone(),
                            message: format!("第{}页: 成功获取视频", page_number),
                            video_id: Some(item.video_id.clone()),
                            view_count: None,
                            favorite_count: None,
                            cover_url,
                        };
                    }
                }
            }

            ScrapeResult {
                success: false,
                name: format!("第{}页", page_number),
                m3u8_url: String::new(),
                message: "未找到可用的视频".to_string(),
                video_id: None,
                view_count: None,
                favorite_count: None,
                cover_url: None,
            }
        })
    }

    fn scrape_all(
        &self,
        page_number: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = Vec<ScrapeResult>> + Send>>
    where
        Self: Sized,
    {
        let page_number = page_number.to_string();
        let log_callback = log_callback.clone();
        let spider = self.clone();

        Box::pin(async move {
            // 1. 用headless_chrome提取视频列表
            let (chrome_logs, video_items) = spider.extract_video_list_with_chrome(&page_number).await;
            for log in chrome_logs {
                log_callback(log);
            }

            let total_count = video_items.len();
            log_callback(format!("开始爬取 {} 个视频...", total_count));

            if video_items.is_empty() {
                return vec![ScrapeResult {
                    success: false,
                    name: format!("第{}页", page_number),
                    m3u8_url: String::new(),
                    message: "未找到视频链接".to_string(),
                    video_id: None,
                    view_count: None,
                    favorite_count: None,
                    cover_url: None,
                }];
            }

            // 2. 并发爬取m3u8
            let (result_tx, mut result_rx) = mpsc::channel::<(String, Option<String>)>(video_items.len());

            for item in &video_items {
                let video_id = item.video_id.clone();
                let spider = spider.clone();
                let result_tx = result_tx.clone();
                let log_tx = log_callback.clone();

                tokio::spawn(async move {
                    let m3u8_url = spider.fetch_m3u8_from_detail(&video_id).await;
                    let _ = result_tx.send((video_id.clone(), m3u8_url.clone())).await;
                });
            }

            // 收集结果
            let mut m3u8_results: Vec<(String, Option<String>)> = Vec::new();
            for _ in 0..video_items.len() {
                if let Some((id, m3u8)) = result_rx.recv().await {
                    m3u8_results.push((id, m3u8));
                }
            }

            // 3. 构建结果
            let mut results: Vec<ScrapeResult> = Vec::new();
            let mut success_count = 0;

            for item in &video_items {
                let m3u8_url = m3u8_results.iter()
                    .find(|(id, _)| id == &item.video_id)
                    .and_then(|(_, url)| url.clone());

                let cover_url = item.cover.clone().filter(|c| c.starts_with("data:image"));

                if let Some(url) = m3u8_url {
                    // 获取标题
                    let video_url = spider.build_url(&format!("/archives/{}.html", item.video_id));
                    let html = match spider.client.get(&video_url).send().await {
                        Ok(resp) => match resp.text().await {
                            Ok(text) => text,
                            Err(_) => String::new(),
                        },
                        Err(_) => String::new(),
                    };
                    let title = spider.extract_title(&html);

                    results.push(ScrapeResult {
                        success: true,
                        name: if title.is_empty() { format!("视频_{}", item.video_id) } else { title },
                        m3u8_url: url,
                        message: "爬取成功".to_string(),
                        video_id: Some(item.video_id.clone()),
                        view_count: None,
                        favorite_count: None,
                        cover_url,
                    });
                    success_count += 1;
                } else {
                    results.push(ScrapeResult {
                        success: false,
                        name: format!("视频_{}", item.video_id),
                        m3u8_url: String::new(),
                        message: "未找到m3u8".to_string(),
                        video_id: Some(item.video_id.clone()),
                        view_count: None,
                        favorite_count: None,
                        cover_url: None,
                    });
                }

                // 短暂延迟
                tokio::time::sleep(Duration::from_millis(200)).await;
            }

            log_callback(format!("完成: 成功 {} / 总数 {}", success_count, total_count));
            results
        })
    }
}
