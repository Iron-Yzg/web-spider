use crate::models::{ScrapeResult, Website};
use reqwest::Client;
use std::future::Future;
use std::pin::Pin;

/// 播放器信息
#[derive(Debug, Clone)]
pub struct PlayerInfo {
    pub video_id: String,
    pub video_type_id: String,
    pub m3u8_urls: Vec<String>,
}

/// SRL爬虫 - 针对 https://wiki.srlqtfff.com/
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

    /// 从页面URL中提取视频ID
    fn extract_video_id(href: &str) -> Option<String> {
        // href format: /archives/203413.html
        href.strip_prefix("/archives/").and_then(|s| {
            s.strip_suffix(".html").map(|s| s.to_string())
        })
    }

    /// 从HTML中提取所有播放器的m3u8 URL
    /// 格式: <div class="dplayer" data-video_id="VIDEOID001" data-video_type_id="ID001">
    fn extract_all_players_from_html(&self, html: &str) -> Vec<PlayerInfo> {
        let mut players: Vec<PlayerInfo> = Vec::new();

        // 匹配所有 dplayer div 及其后续的 m3u8 URLs
        // 模式: <div class="dplayer" data-video_id="XXX" data-video_type_id="XXX"> ... m3u8 ...
        let dplayer_pattern = regex::Regex::new(
            r#"<div\s+class="dplayer"\s+[^>]*data-video_id="([^"]*)"[^>]*data-video_type_id="([^"]*)"[^>]*>"#
        ).unwrap();

        // 提取所有m3u8 URL
        let m3u8_pattern = regex::Regex::new(r#""([^"]+\.m3u8[^"]*)""#).unwrap();
        let all_m3u8s: Vec<String> = m3u8_pattern
            .captures_iter(html)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();

        if all_m3u8s.is_empty() {
            eprintln!("[DEBUG] 未找到任何m3u8 URL");
            return players;
        }

        eprintln!("[DEBUG] 找到 {} 个m3u8 URL", all_m3u8s.len());

        // 匹配dplayer元素
        for cap in dplayer_pattern.captures_iter(html) {
            let video_id = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
            let video_type_id = cap.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();

            eprintln!("[DEBUG] 发现播放器: video_id={}, video_type_id={}", video_id, video_type_id);

            // 为每个播放器分配一个m3u8 URL（按顺序）
            let idx = players.len();
            let m3u8_url = if idx < all_m3u8s.len() {
                let url = all_m3u8s[idx].clone();
                // 处理URL前缀
                if url.starts_with("//") {
                    format!("https:{}", url)
                } else if url.starts_with("/") {
                    format!("https://wiki.srlqtfff.com{}", url)
                } else {
                    url
                }
            } else {
                // 如果播放器比m3u8多，使用最后一个
                let url = all_m3u8s.last().unwrap().clone();
                if url.starts_with("//") {
                    format!("https:{}", url)
                } else if url.starts_with("/") {
                    format!("https://wiki.srlqtfff.com{}", url)
                } else {
                    url
                }
            };

            players.push(PlayerInfo {
                video_id,
                video_type_id,
                m3u8_urls: vec![m3u8_url],
            });
        }

        // 如果没有找到dplayer元素，但有m3u8，使用索引作为ID
        if players.is_empty() && !all_m3u8s.is_empty() {
            for (i, m3u8) in all_m3u8s.into_iter().enumerate() {
                let url = if m3u8.starts_with("//") {
                    format!("https:{}", m3u8)
                } else if m3u8.starts_with("/") {
                    format!("https://wiki.srlqtfff.com{}", m3u8)
                } else {
                    m3u8
                };
                players.push(PlayerInfo {
                    video_id: format!("player_{}", i + 1),
                    video_type_id: format!("{}", i + 1),
                    m3u8_urls: vec![url],
                });
            }
        }

        players
    }

    /// 从HTML中提取m3u8 URL（兼容旧接口，返回第一个）
    fn extract_m3u8_from_html(&self, html: &String) -> String {
        let players = self.extract_all_players_from_html(html);
        if let Some(first) = players.first() {
            if let Some(url) = first.m3u8_urls.first() {
                return url.clone();
            }
        }
        "".to_string()
    }

    /// 从HTML中提取标题
    fn extract_title_from_html(&self, html: &str) -> String {
        // 匹配 <h1 class="post-title " itemprop="name headline">...</h1>
        let title_pattern = regex::Regex::new(r#"<h1\s+class="post-title\s*"[^>]*itemprop="name headline"[^>]*>(.*?)</h1>"#).unwrap();
        if let Some(cap) = title_pattern.captures(html).and_then(|cap| cap.get(1)) {
            let title = cap.as_str();
            // 清理HTML标签
            let clean_pattern = regex::Regex::new(r#"<[^>]+>"#).unwrap();
            let cleaned = clean_pattern.replace_all(title, " ");
            // 清理空白字符
            let result: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
            return result.trim().to_string();
        }
        // 备选：尝试匹配其他可能的选择器
        let alt_pattern = regex::Regex::new(r#"<h1\s+class="post-title"[^>]*>(.*?)</h1>"#).unwrap();
        if let Some(cap) = alt_pattern.captures(html).and_then(|cap| cap.get(1)) {
            let title = cap.as_str();
            let clean_pattern = regex::Regex::new(r#"<[^>]+>"#).unwrap();
            let cleaned = clean_pattern.replace_all(title, " ");
            let result: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
            return result.trim().to_string();
        }
        "".to_string()
    }

    /// 从列表页提取所有视频ID
    fn extract_video_ids_from_list(&self, html: &str) -> Vec<String> {
        let mut video_ids: Vec<String> = Vec::new();

        // 首先尝试从 article 标签中提取
        let article_pattern = regex::Regex::new(r#"<article[^>]*>.*?href="/archives/(\d+)\.html".*?</article>"#).unwrap();
        for cap in article_pattern.captures_iter(html) {
            if let Some(id_cap) = cap.get(1) {
                let id = id_cap.as_str().to_string();
                if !video_ids.contains(&id) {
                    video_ids.push(id);
                }
            }
        }

        // 备选：从所有链接中提取
        if video_ids.is_empty() {
            let link_pattern = regex::Regex::new(r#"href="/archives/(\d+)\.html""#).unwrap();
            for cap in link_pattern.captures_iter(html) {
                if let Some(id_cap) = cap.get(1) {
                    let id = id_cap.as_str().to_string();
                    if !video_ids.contains(&id) {
                        video_ids.push(id);
                    }
                }
            }
        }

        video_ids
    }
}

impl crate::services::Scraper for SrlSpider {
    fn id(&self) -> &'static str {
        "srl"
    }

    fn scrape(
        &self,
        page_number: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = ScrapeResult> + Send>> {
        let page_number = page_number.to_string();
        let website = self.website.clone();
        let client = self.client.clone();
        let log_callback = log_callback.clone();

        Box::pin(async move {
            let page_url = format!("https://wiki.srlqtfff.com/page/{}", page_number);
            let _ = log_callback(format!("访问列表页: {}", page_url));

            // 解析列表页
            let response = match client.get(&page_url).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    return ScrapeResult {
                        success: false,
                        name: format!("第{}页", page_number),
                        m3u8_url: String::new(),
                        message: format!("请求失败: {}", e),
                        video_id: None,
                    };
                }
            };

            if !response.status().is_success() {
                return ScrapeResult {
                    success: false,
                    name: format!("第{}页", page_number),
                    m3u8_url: String::new(),
                    message: format!("请求失败: HTTP {}", response.status()),
                    video_id: None,
                };
            }

            let html = match response.text().await {
                Ok(text) => text,
                Err(e) => {
                    return ScrapeResult {
                        success: false,
                        name: format!("第{}页", page_number),
                        m3u8_url: String::new(),
                        message: format!("读取响应失败: {}", e),
                        video_id: None,
                    };
                }
            };

            // 提取视频ID列表
            let video_links = SrlSpider { website: website.clone(), client: client.clone() }
                .extract_video_ids_from_list(&html);

            let _ = log_callback(format!("找到 {} 个视频链接", video_links.len()));

            if video_links.is_empty() {
                return ScrapeResult {
                    success: false,
                    name: format!("第{}页", page_number),
                    m3u8_url: String::new(),
                    message: "未找到视频链接".to_string(),
                    video_id: None,
                };
            }

            // 爬取每个视频
            let mut results: Vec<ScrapeResult> = Vec::new();
            let mut success_count = 0;
            let spider = SrlSpider { website: website.clone(), client: client.clone() };

            for (i, video_id) in video_links.iter().enumerate() {
                let _ = log_callback(format!("[{}] 爬取视频: {}", i + 1, video_id));

                let video_url = format!("https://wiki.srlqtfff.com/archives/{}.html", video_id);
                let response = client.get(&video_url).send().await;

                match response {
                    Ok(resp) if resp.status().is_success() => {
                        let video_html = resp.text().await.unwrap_or_default();

                        // 解析标题
                        let video_name = spider.extract_title_from_html(&video_html);

                        // 提取m3u8
                        let m3u8_url = spider.extract_m3u8_from_html(&video_html);

                        if !m3u8_url.is_empty() {
                            results.push(ScrapeResult {
                                success: true,
                                name: if video_name.is_empty() { format!("视频_{}", video_id) } else { video_name },
                                m3u8_url,
                                message: "爬取成功".to_string(),
                                video_id: Some(video_id.clone()),
                            });
                            success_count += 1;
                        } else {
                            results.push(ScrapeResult {
                                success: false,
                                name: if video_name.is_empty() { format!("视频_{}", video_id) } else { video_name },
                                m3u8_url: String::new(),
                                message: "未找到m3u8地址".to_string(),
                                video_id: Some(video_id.clone()),
                            });
                        }
                    }
                    _ => {
                        results.push(ScrapeResult {
                            success: false,
                            name: format!("视频_{}", video_id),
                            m3u8_url: String::new(),
                            message: "请求失败".to_string(),
                            video_id: Some(video_id.clone()),
                        });
                    }
                }

                // 短暂延迟避免请求过快
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }

            let _ = log_callback(format!("完成: 成功 {} / 总数 {}", success_count, video_links.len()));

            // 返回第一个成功的视频作为主要结果
            if let Some(first_result) = results.into_iter().find(|r| r.success) {
                ScrapeResult {
                    success: true,
                    name: first_result.name,
                    m3u8_url: first_result.m3u8_url,
                    message: format!("第{}页: 成功爬取 {} 个视频", page_number, success_count),
                    video_id: first_result.video_id.clone(),
                }
            } else {
                ScrapeResult {
                    success: false,
                    name: format!("第{}页", page_number),
                    m3u8_url: String::new(),
                    message: format!("未找到可用的视频 (成功{}/{})", success_count, video_links.len()),
                    video_id: None,
                }
            }
        })
    }

    /// 爬取所有视频，每个视频单独保存
    fn scrape_all(
        &self,
        page_number: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = Vec<ScrapeResult>> + Send>>
    where
        Self: Sized,
    {
        let page_number = page_number.to_string();
        let website = self.website.clone();
        let client = self.client.clone();
        let log_callback = log_callback.clone();

        Box::pin(async move {
            let page_url = format!("https://wiki.srlqtfff.com/page/{}", page_number);
            let _ = log_callback(format!("访问列表页: {}", page_url));

            // 解析列表页
            let response = match client.get(&page_url).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    return vec![ScrapeResult {
                        success: false,
                        name: format!("第{}页", page_number),
                        m3u8_url: String::new(),
                        message: format!("请求失败: {}", e),
                        video_id: None,
                    }];
                }
            };

            if !response.status().is_success() {
                return vec![ScrapeResult {
                    success: false,
                    name: format!("第{}页", page_number),
                    m3u8_url: String::new(),
                    message: format!("请求失败: HTTP {}", response.status()),
                    video_id: None,
                }];
            }

            let html = match response.text().await {
                Ok(text) => text,
                Err(e) => {
                    return vec![ScrapeResult {
                        success: false,
                        name: format!("第{}页", page_number),
                        m3u8_url: String::new(),
                        message: format!("读取响应失败: {}", e),
                        video_id: None,
                    }];
                }
            };

            // 提取视频ID列表
            let video_links = SrlSpider { website: website.clone(), client: client.clone() }
                .extract_video_ids_from_list(&html);

            let total_count = video_links.len();
            let _ = log_callback(format!("找到 {} 个视频链接，开始并发爬取...", total_count));

            if video_links.is_empty() {
                return vec![ScrapeResult {
                    success: false,
                    name: format!("第{}页", page_number),
                    m3u8_url: String::new(),
                    message: "未找到视频链接".to_string(),
                    video_id: None,
                }];
            }

            // 并发爬取每个视频
            let mut tasks = Vec::new();
            for (i, video_id) in video_links.iter().enumerate() {
                let video_id = video_id.clone();
                let client = client.clone();
                let log_callback = log_callback.clone();
                let website = website.clone();

                let task = tokio::spawn(async move {
                    let result = scrape_single_video(
                        &client,
                        &website,
                        &video_id,
                        i + 1,
                        &log_callback
                    ).await;
                    result
                });

                tasks.push(task);
            }

            // 等待所有任务完成
            let mut results: Vec<ScrapeResult> = Vec::new();
            let mut success_count = 0;

            for task in tasks {
                match task.await {
                    Ok(task_results) => {
                        for r in task_results {
                            results.push(r.clone());
                            if r.success {
                                success_count += 1;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = log_callback(format!("任务执行错误: {}", e));
                    }
                }
            }

            let _ = log_callback(format!("完成: 成功 {} / 总数 {}", success_count, total_count));

            results
        })
    }
}

/// 并发爬取单个视频页面及其所有播放器
async fn scrape_single_video(
    client: &Client,
    website: &Website,
    video_id: &str,
    index: usize,
    log_callback: &(impl Fn(String) + Clone),
) -> Vec<ScrapeResult> {
    let mut results: Vec<ScrapeResult> = Vec::new();

    let _ = log_callback(format!("[{}] 爬取视频: {}", index, video_id));

    let video_url = format!("https://wiki.srlqtfff.com/archives/{}.html", video_id);

    match client.get(&video_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let video_html = resp.text().await.unwrap_or_default();

            // 解析标题
            let spider = SrlSpider::new(website);
            let video_name = spider.extract_title_from_html(&video_html);

            // 提取所有播放器信息
            let players = spider.extract_all_players_from_html(&video_html);

            if players.is_empty() {
                results.push(ScrapeResult {
                    success: false,
                    name: if video_name.is_empty() { format!("视频_{}", video_id) } else { video_name.clone() },
                    m3u8_url: String::new(),
                    message: "未找到播放器".to_string(),
                    video_id: Some(video_id.to_string()),
                });
                let _ = log_callback(format!("  ✗ 未找到播放器: {}", video_id));
            } else {
                // 为每个播放器创建结果
                for (player_idx, player) in players.iter().enumerate() {
                    for (_url_idx, m3u8_url) in player.m3u8_urls.iter().enumerate() {
                        if m3u8_url.is_empty() {
                            continue;
                        }

                        // 构建视频名称
                        let name = if players.len() > 1 {
                            if !video_name.is_empty() {
                                format!("{} (第{}部分)", video_name, player_idx + 1)
                            } else {
                                format!("视频_{}_part{}", video_id, player_idx + 1)
                            }
                        } else if !video_name.is_empty() {
                            video_name.clone()
                        } else {
                            format!("视频_{}", video_id)
                        };

                        // 构建唯一的视频ID
                        let unique_video_id = if player.video_type_id.is_empty() {
                            format!("{}_{}", video_id, player_idx + 1)
                        } else {
                            format!("{}_{}", video_id, player.video_type_id)
                        };

                        results.push(ScrapeResult {
                            success: true,
                            name: name.clone(),
                            m3u8_url: m3u8_url.clone(),
                            message: format!("第{}个播放器", player_idx + 1),
                            video_id: Some(unique_video_id.clone()),
                        });

                        let _ = log_callback(format!("  ✓ [{}] 成功: {} ({})", player_idx + 1, name, m3u8_url));
                    }
                }

                if players.len() > 1 {
                    let _ = log_callback(format!("  📺 页面包含 {} 个播放器", players.len()));
                }
            }
        }
        Ok(resp) => {
            results.push(ScrapeResult {
                success: false,
                name: format!("视频_{}", video_id),
                m3u8_url: String::new(),
                message: format!("HTTP错误: {}", resp.status()),
                video_id: Some(video_id.to_string()),
            });
            let _ = log_callback(format!("  ✗ HTTP错误 {}: video_{}", resp.status(), video_id));
        }
        Err(e) => {
            results.push(ScrapeResult {
                success: false,
                name: format!("视频_{}", video_id),
                m3u8_url: String::new(),
                message: format!("请求失败: {}", e),
                video_id: Some(video_id.to_string()),
            });
            let _ = log_callback(format!("  ✗ 请求失败: video_{} - {}", video_id, e));
        }
    }

    results
}
