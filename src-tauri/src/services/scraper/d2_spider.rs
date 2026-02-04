use crate::models::{LocalStorageItem, ScrapeResult, Website};
use crate::services::scraper::Scraper;
use headless_chrome::Browser;
use regex::Regex;
use std::ffi::OsStr;
use std::pin::Pin;
use std::time::Duration;
use std::future::Future;

/// D2 Cloudfront 爬虫 - 专门爬取 d1ibyof3mbdf0n.cloudfront.net 列表页
#[derive(Clone)]
pub struct D2Spider {
    base_url: String,
    local_storage: Vec<LocalStorageItem>,
}

impl D2Spider {
    pub fn new(website: &Website) -> Self {
        Self {
            base_url: website.base_url.clone(),
            local_storage: website.local_storage.clone(),
        }
    }
}

/// 从页面HTML中提取视频列表信息（不包含m3u8）
fn extract_videos_from_html(html: &str) -> Vec<VideoInfo> {
    let mut videos: Vec<VideoInfo> = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    // 清理HTML中的多余空白，方便解析
    let clean_html = html.replace("\n", "").replace("\r", "").replace("> <", "><");

    // 匹配视频卡片结构 - 使用 card-item 作为边界
    // 每个卡片包含: longVideoCard (图片+时长) + title(标题) + tags-box(收藏数)
    let card_pattern = Regex::new(
        r#"<div[^>]*class="[^"]*card-item[^"]*"[^>]*>([\s\S]*?)<div[^>]*class="[^"]*longVideoCard[^"]*"[^>]*>([\s\S]*?)</div>[\s]*<div[^>]*class="[^"]*"[^>]*>[\s]*<div[^>]*class="[^"]*title[^"]*"[^>]*>[\s]*<p[^>]*>([^<]+)</p>[\s]*</div>[\s]*<div[^>]*class="[^"]*tags-box[^"]*"[^>]*>([\s\S]*?)</div>"#
    ).unwrap();

    // 从 longVideoCard 中提取封面 - 优先使用 data-src，其次是 src
    // Vue可能使用 :src 绑定，渲染后可能是 data-src 或 src
    let cover_pattern = Regex::new(r#"<img[^>]*class="[^"]*wh-full[^"]*d-block[^"]*"[^>]*data-src="([^"]*)"[^>]*"#).unwrap();
    let cover_pattern2 = Regex::new(r#"<img[^>]*class="[^"]*wh-full[^"]*d-block[^"]*"[^>]*src="([^"]*)"[^>]*"#).unwrap();
    let cover_pattern3 = Regex::new(r#"<img[^>]*class="[^"]*wh-full[^"]*d-block[^"]*"[^>]*:src="([^"]*)"[^>]*"#).unwrap();
    let _cover_pattern4 = Regex::new(r#"<img[^>]*class="[^"]*wh-full[^"]*d-block[^"]*"[^>]*srcset="[^"]*"[^>]*"#).unwrap();

    let duration_pattern = Regex::new(r#"<div[^>]*class="[^"]*collectPack[^"]*"[^>]*>(\d{1,2}:\d{2}:\d{2})</div>"#).unwrap();

    // 从 video-time 区域提取播放数（第一个 collectPack）
    let views_in_time_pattern = Regex::new(r#"<div[^>]*class="[^"]*video-time[^"]*"[^>]*>[\s]*<div[^>]*class="[^"]*collectPack[^"]*"[^>]*>(?:<img[^>]*>)?[\s]*(\d+\.?\d*[万亿]?)[\s]*</div>"#).unwrap();

    // 从 tags-box 区域提取收藏数
    let fav_in_tags_pattern = Regex::new(r#"收藏数\s*(\d+)"#).unwrap();

    tracing::info!("[DEBUG] Starting extraction...");

    // 使用卡片边界来提取，避免重复
    for card_cap in card_pattern.captures_iter(&clean_html) {
        let _card_content = card_cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let long_video_card = card_cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let name = card_cap.get(3).map(|m| m.as_str().to_string()
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&nbsp;", " ")
            .trim()
            .to_string()
        ).unwrap_or_default();
        let tags_box = card_cap.get(4).map(|m| m.as_str()).unwrap_or("");

        // 跳过空标题或太短的标题
        if name.is_empty() || name.len() < 2 {
            continue;
        }

        // 去重
        if seen_names.contains(&name) {
            continue;
        }
        seen_names.insert(name.clone());

        // 提取封面 - 尝试多种模式
        let cover_url = {
            // 先尝试 data-src
            if let Some(cap) = cover_pattern.captures(long_video_card) {
                let url = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                if !url.is_empty() {
                    process_cover_url(&url)
                } else {
                    String::new()
                }
            } else if let Some(cap) = cover_pattern3.captures(long_video_card) {
                // 尝试 :src (Vue绑定)
                let url = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                if !url.is_empty() {
                    process_cover_url(&url)
                } else {
                    String::new()
                }
            } else if let Some(cap) = cover_pattern2.captures(long_video_card) {
                // 尝试 src
                let url = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                if !url.is_empty() {
                    process_cover_url(&url)
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        };

        // 提取时长
        let duration = duration_pattern.captures(long_video_card)
            .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .unwrap_or_default();

        // 提取播放数（从 video-time 区域）
        let views = views_in_time_pattern.captures(long_video_card)
            .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .unwrap_or_default();

        // 提取收藏数
        let favorite_count = fav_in_tags_pattern.captures(tags_box)
            .and_then(|cap| cap.get(1).map(|m| m.as_str().parse::<i64>().unwrap_or(0)))
            .unwrap_or(0);

        // 生成视频ID
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let video_id = format!("{:x}", hasher.finish());

        tracing::info!("[DEBUG] Extracted: name='{}', views='{}', duration='{}', fav={}, cover='{}'",
            name, views, duration, favorite_count,
            if cover_url.is_empty() { "(empty)" } else { &cover_url[..cover_url.len().min(50)] });

        videos.push(VideoInfo {
            id: video_id,
            name,
            _cover_url: cover_url,
            _m3u8_url: String::new(),
            _duration: duration,
            favorite_count,
            view_count: Some(parse_view_count(&views)),
            _tag: String::new(),
        });
    }

    // 如果主模式没有找到，尝试更宽松的提取方式
    if videos.is_empty() {
        tracing::info!("[DEBUG] Main pattern found nothing, trying fallback...");

        // 匹配 longVideoCard 块及其后的标题
        let alt_card_pattern = Regex::new(
            r#"<div[^>]*class="[^"]*longVideoCard[^"]*"[^>]*>([\s\S]*?)</div>[\s]*<div[^>]*>[\s]*<div[^>]*class="[^"]*title[^"]*"[^>]*>[\s]*<p[^>]*>([^<]+)</p>[\s]*</div>[\s]*<div[^>]*class="[^"]*tags-box[^"]*"[^>]*>([\s\S]*?)</div>"#
        ).unwrap();

        for cap in alt_card_pattern.captures_iter(&clean_html) {
            let card_content = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let name = cap.get(2).map(|m| m.as_str().to_string()
                .replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&#39;", "'")
                .replace("&nbsp;", " ")
                .trim()
                .to_string()
            ).unwrap_or_default();
            let tags_box = cap.get(3).map(|m| m.as_str()).unwrap_or("");

            if name.is_empty() || name.len() < 2 {
                continue;
            }

            if seen_names.contains(&name) {
                continue;
            }
            seen_names.insert(name.clone());

            // 提取封面 - 尝试多种模式
            let cover_url = {
                if let Some(cap) = cover_pattern.captures(card_content) {
                    let url = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                    if !url.is_empty() {
                        process_cover_url(&url)
                    } else {
                        String::new()
                    }
                } else if let Some(cap) = cover_pattern3.captures(card_content) {
                    let url = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                    if !url.is_empty() {
                        process_cover_url(&url)
                    } else {
                        String::new()
                    }
                } else if let Some(cap) = cover_pattern2.captures(card_content) {
                    let url = cap.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                    if !url.is_empty() {
                        process_cover_url(&url)
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            };

            // 提取时长
            let duration = duration_pattern.captures(card_content)
                .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                .unwrap_or_default();

            // 提取播放数 - 在 collectPack 中找数字
            let views_numeric_pattern = Regex::new(r#"<div[^>]*class="[^"]*collectPack[^"]*"[^>]*>(?:<img[^>]*>)?[\s]*(\d+\.?\d*[万亿]?)[\s]*</div>"#).unwrap();
            let views = views_numeric_pattern.captures(card_content)
                .and_then(|cap| cap.get(1).map(|m| m.as_str().to_string()))
                .unwrap_or_default();

            // 提取收藏数
            let favorite_count = fav_in_tags_pattern.captures(tags_box)
                .and_then(|cap| cap.get(1).map(|m| m.as_str().parse::<i64>().unwrap_or(0)))
                .unwrap_or(0);

            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            name.hash(&mut hasher);
            let video_id = format!("{:x}", hasher.finish());

            tracing::info!("[DEBUG] Alt extracted: name='{}', views='{}', duration='{}', fav={}, cover='{}'",
                name, views, duration, favorite_count,
                if cover_url.is_empty() { "(empty)" } else { &cover_url[..cover_url.len().min(50)] });

            videos.push(VideoInfo {
                id: video_id,
                name,
                _cover_url: cover_url,
                _m3u8_url: String::new(),
                _duration: duration,
                favorite_count,
                view_count: Some(parse_view_count(&views)),
                _tag: String::new(),
            });
        }
    }

    // 最终去重：基于视频ID去重
    let mut final_videos: Vec<VideoInfo> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for video in videos {
        if !seen_ids.contains(&video.id) {
            seen_ids.insert(video.id.clone());
            final_videos.push(video);
        }
    }

    tracing::info!("[DEBUG] Total videos extracted: {} (after dedup)", final_videos.len());
    final_videos
}

/// 处理封面URL，补全协议
fn process_cover_url(url: &str) -> String {
    if url.is_empty() {
        return String::new();
    }

    let url = url.trim();

    if url.starts_with("//") {
        format!("https:{}", url)
    } else if url.starts_with("http://") {
        url.to_string()
    } else if url.starts_with("https://") {
        url.to_string()
    } else if url.starts_with("/") {
        // 相对路径，保持不变
        url.to_string()
    } else {
        // 可能是相对路径
        url.to_string()
    }
}

/// 解析播放数（支持万、亿单位）
fn parse_view_count(views: &str) -> i64 {
    let views = views.trim();
    if views.is_empty() {
        return 0;
    }

    let multiplier = if views.ends_with('亿') {
        100_000_000
    } else if views.ends_with('万') {
        10_000
    } else {
        1
    };

    let num_str = if multiplier > 1 {
        &views[..views.len() - 1]
    } else {
        views
    };

    let num_str = num_str.split('.').next().unwrap_or(num_str);
    num_str.parse::<i64>().unwrap_or(0) * multiplier
}

impl Scraper for D2Spider {
    fn id(&self) -> &'static str {
        "d2"
    }

    fn scrape(
        &self,
        _video_id: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = ScrapeResult> + Send>> {
        let base_url = self.base_url.clone();
        let local_storage = self.local_storage.clone();
        let log_callback = log_callback.clone();

        Box::pin(async move {
            let page_url = format!("{}", base_url);
            let _ = log_callback(format!("正在爬取: {}", page_url));

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
                    return ScrapeResult {
                        success: false,
                        name: String::new(),
                        m3u8_url: String::new(),
                        message: format!("启动浏览器失败: {}", e),
                        video_id: None,
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
                        video_id: None,
                        view_count: None,
                        favorite_count: None,
                        cover_url: None,
                    };
                }
            };

            // 先导航到 about:blank，注入 localStorage 后再跳转到目标页面
            let _ = tab.navigate_to("about:blank");
            tokio::time::sleep(Duration::from_millis(500)).await;

            // 注入 localStorage（必须在页面加载前）
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
                let _ = log_callback(format!("已注入 {} 个 localStorage 项", local_storage.len()));
            }

            // 导航到目标页面
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
                    video_id: None,
                    view_count: None,
                    favorite_count: None,
                    cover_url: None,
                };
            }

            // 等待页面加载
            tokio::time::sleep(Duration::from_secs(3)).await;

            // 第一步：关闭弹窗广告 - 使用多种方式
            let close_popups_js = r#"
                (function() {
                    // 1. 移除所有遮罩层
                    document.querySelectorAll('.van-overlay, .van-popup, .van-dialog, .van-modal').forEach(el => {
                        el.style.display = 'none';
                        el.remove();
                    });

                    // 2. 点击所有"跳过"、"关闭"、"知道了"、"取消"按钮
                    document.querySelectorAll('button, .van-button').forEach(btn => {
                        const text = btn.innerText || btn.textContent || '';
                        if (text.includes('跳过') || text.includes('关闭') || text.includes('知道了') || text.includes('取消') || text.includes('确定')) {
                            btn.click();
                        }
                    });

                    // 3. 点击所有带有close类的元素
                    document.querySelectorAll('[class*="close"], [class*="Close"]').forEach(el => {
                        el.click();
                    });
                })();
            "#;
            let _ = tab.evaluate(close_popups_js, false);
            tokio::time::sleep(Duration::from_millis(500)).await;

            // 第二步：注入 CSS 隐藏剩余的广告和弹窗 - 使用油猴脚本的选择器
            let hide_ad_css = r#"
                (function() {
                    const selectors = [
                        '.vue-nice-modal-root', 'div.preview-ui', 'div.skip-preview-btn',
                        'div.mine-ad', 'div.openvip', 'div.van-popup', 'div.van-overlay',
                        'div.function-grid', 'div.layout-notice-swiper', 'div.promotion-expire',
                        'div.bottom-link', 'div.first-comment:nth-child(1)', 'div.sub-nav',
                        'div.card-item.mb-5:has(.bannerCover)', 'img[data-v-]',
                        'div.van-tabbar-item:nth-child(4)', 'div.van-tabbar-item:nth-child(6)',
                        'div.van-tab--shrink:nth-child(2)', 'div.van-tab--shrink:nth-child(3)', 'div.van-tab--shrink:nth-child(4)',
                        '.bannerCover', '.banner-container', 'div.mine-ad', 'div.openvip',
                        'div.skip-preview-btn', 'div.preview-ui', '.preview-modal',
                        '.van-dialog', '.van-modal', '[class*="advertisement"]',
                        '[class*="ad-"]', '[id*="ad-"]', '.welcome-modal', '.notice-modal'
                    ].join(', ');
                    const style = document.createElement('style');
                    style.textContent = selectors + ' { display: none !important; visibility: hidden !important; pointer-events: none !important; opacity: 0 !important; }';
                    (document.head || document.documentElement).appendChild(style);
                })();
            "#;
            let _ = tab.evaluate(hide_ad_css, false);
            tokio::time::sleep(Duration::from_millis(500)).await;

            // 第三步：模拟按ESC键关闭任何剩余弹窗
            let _ = tab.evaluate("document.dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape', keyCode: 27}))", false);
            tokio::time::sleep(Duration::from_millis(500)).await;

            // 第四步：再次检查并关闭弹窗
            let _ = tab.evaluate(close_popups_js, false);
            tokio::time::sleep(Duration::from_millis(500)).await;

            let _ = log_callback(String::from("正在等待页面渲染..."));

            // 等待 Vue 应用渲染（最多 8 秒）
            for i in 0..40 {
                tokio::time::sleep(Duration::from_millis(200)).await;

                // 检查是否已有视频卡片
                let has_card: bool = tab.evaluate("document.querySelector('.card-item, .longVideoCard') !== null", false)
                    .map(|r| r.value.unwrap_or_default().as_bool().unwrap_or(false))
                    .unwrap_or(false);

                // 检查页面内容是否足够多（Vue 已渲染）
                let body_text_len: i64 = tab.evaluate("document.body.innerText.length", false)
                    .map(|r| r.value.unwrap_or_default().as_i64().unwrap_or(0))
                    .unwrap_or(0);

                if has_card && body_text_len > 1000 {
                    let _ = log_callback(format!("检测到视频卡片 (等待 {}ms)", i * 200));
                    break;
                }

                // 每 10 次检查输出日志
                if i % 10 == 0 {
                    let _ = log_callback(format!("等待中... body长度: {}", body_text_len));
                }
            }

            // 再次关闭弹窗（确保页面加载完成后弹窗也被关闭）
            let _ = tab.evaluate(close_popups_js, false);
            tokio::time::sleep(Duration::from_millis(300)).await;

            // 最后再注入一次 CSS 隐藏
            let _ = tab.evaluate(hide_ad_css, false);

            // 再等待一下确保渲染完成
            tokio::time::sleep(Duration::from_secs(1)).await;

            // 获取页面HTML
            let html: String = match tab.evaluate("document.documentElement.outerHTML", false) {
                Ok(result) => result.value.unwrap_or_default().as_str().unwrap_or("").to_string(),
                Err(e) => {
                    let _ = tab.close(true);
                    drop(tab);
                    drop(browser);
                    return ScrapeResult {
                        success: false,
                        name: String::new(),
                        m3u8_url: String::new(),
                        message: format!("获取页面HTML失败: {}", e),
                        video_id: None,
                        view_count: None,
                        favorite_count: None,
                        cover_url: None,
                    };
                }
            };

            // 调试输出：检查HTML内容
            tracing::info!("[DEBUG] HTML length: {} bytes", html.len());
            tracing::info!("[DEBUG] HTML preview (first 2000 chars):\n{}", &html);

            // 检查是否有 card-item 或 longVideoCard 类
            let has_card_item = html.contains("card-item");
            let has_long_video_card = html.contains("longVideoCard");
            let has_collect_pack = html.contains("collectPack");
            let has_video_time = html.contains("video-time");
            let has_wh_full = html.contains("wh-full");

            tracing::info!("[DEBUG] Contains 'card-item': {}", has_card_item);
            tracing::info!("[DEBUG] Contains 'longVideoCard': {}", has_long_video_card);
            tracing::info!("[DEBUG] Contains 'collectPack': {}", has_collect_pack);
            tracing::info!("[DEBUG] Contains 'video-time': {}", has_video_time);
            tracing::info!("[DEBUG] Contains 'wh-full': {}", has_wh_full);

            // 提取视频列表（使用独立函数）
            let videos = extract_videos_from_html(&html);

            let _ = log_callback(format!("找到 {} 个视频", videos.len()));

            if videos.is_empty() {
                let _ = tab.close(true);
                drop(tab);
                drop(browser);
                return ScrapeResult {
                    success: false,
                    name: "未找到视频".to_string(),
                    m3u8_url: String::new(),
                    message: "页面中未找到视频卡片".to_string(),
                    video_id: None,
                    view_count: None,
                    favorite_count: None,
                    cover_url: None,
                };
            }

            // 关闭浏览器
            let _ = tab.close(true);
            drop(tab);
            drop(browser);

            // 返回第一个视频作为主要结果
            let first_video = &videos[0];
            ScrapeResult {
                success: true,
                name: first_video.name.clone(),
                m3u8_url: first_video._m3u8_url.clone(),
                message: format!("找到 {} 个视频 (点击卡片获取m3u8)", videos.len()),
                video_id: Some(first_video.id.clone()),
                view_count: first_video.view_count,
                favorite_count: Some(first_video.favorite_count),
                cover_url: None,
            }
        })
    }

    fn scrape_all(
        &self,
        _video_id: &str,
        log_callback: impl Fn(String) + Clone + Send + Sync + 'static,
    ) -> Pin<Box<dyn Future<Output = Vec<ScrapeResult>> + Send + 'static>>
    where
        Self: Sized,
    {
        let base_url = self.base_url.clone();
        let local_storage = self.local_storage.clone();
        let log_callback = log_callback.clone();

        Box::pin(async move {
            let page_url = format!("{}", base_url);
            let _ = log_callback(format!("正在爬取: {}", page_url));

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
                    return vec![ScrapeResult {
                        success: false,
                        name: String::new(),
                        m3u8_url: String::new(),
                        message: format!("启动浏览器失败: {}", e),
                        video_id: None,
                        view_count: None,
                        favorite_count: None,
                        cover_url: None,
                    }];
                }
            };

            let tab = match browser.new_tab() {
                Ok(tab) => tab,
                Err(e) => {
                    return vec![ScrapeResult {
                        success: false,
                        name: String::new(),
                        m3u8_url: String::new(),
                        message: format!("创建标签页失败: {}", e),
                        video_id: None,
                        view_count: None,
                        favorite_count: None,
                        cover_url: None,
                    }];
                }
            };

            // 先导航到 about:blank，注入 localStorage 后再跳转到目标页面
            let _ = tab.navigate_to("about:blank");
            tokio::time::sleep(Duration::from_millis(500)).await;

            // 注入 localStorage（必须在页面加载前）
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
                let _ = log_callback(format!("已注入 {} 个 localStorage 项", local_storage.len()));
            }

            // 导航到目标页面（带重试）
            let mut nav_success = false;
            for attempt in 1..=3 {
                match tab.navigate_to(&page_url) {
                    Ok(_) => {
                        nav_success = true;
                        break;
                    }
                    Err(_) => {
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
                return vec![ScrapeResult {
                    success: false,
                    name: String::new(),
                    m3u8_url: String::new(),
                    message: "导航失败".to_string(),
                    video_id: None,
                    view_count: None,
                    favorite_count: None,
                    cover_url: None,
                }];
            }

            // 等待页面加载
            tokio::time::sleep(Duration::from_secs(3)).await;

            // 关闭弹窗广告
            let close_popups_js = r#"
                (function() {
                    document.querySelectorAll('.van-overlay, .van-popup, .van-dialog, .van-modal').forEach(el => {
                        el.style.display = 'none';
                        el.remove();
                    });
                    document.querySelectorAll('button, .van-button').forEach(btn => {
                        const text = btn.innerText || btn.textContent || '';
                        if (text.includes('跳过') || text.includes('关闭') || text.includes('知道了') || text.includes('取消') || text.includes('确定')) {
                            btn.click();
                        }
                    });
                    document.querySelectorAll('[class*="close"], [class*="Close"]').forEach(el => {
                        el.click();
                    });
                })();
            "#;
            let _ = tab.evaluate(close_popups_js, false);
            tokio::time::sleep(Duration::from_millis(500)).await;

            // 注入 CSS 隐藏广告和弹窗
            let hide_ad_css = r#"
                (function() {
                    const selectors = [
                        '.vue-nice-modal-root', 'div.preview-ui', 'div.skip-preview-btn',
                        'div.mine-ad', 'div.openvip', 'div.van-popup', 'div.van-overlay',
                        'div.function-grid', 'div.layout-notice-swiper', 'div.promotion-expire',
                        'div.bottom-link', 'div.first-comment:nth-child(1)', 'div.sub-nav',
                        'div.card-item.mb-5:has(.bannerCover)', 'img[data-v-]',
                        'div.van-tabbar-item:nth-child(4)', 'div.van-tabbar-item:nth-child(6)',
                        'div.van-tab--shrink:nth-child(2)', 'div.van-tab--shrink:nth-child(3)', 'div.van-tab--shrink:nth-child(4)',
                        '.bannerCover', '.banner-container', 'div.mine-ad', 'div.openvip',
                        'div.skip-preview-btn', 'div.preview-ui', '.preview-modal',
                        '.van-dialog', '.van-modal', '[class*="advertisement"]',
                        '[class*="ad-"]', '[id*="ad-"]', '.welcome-modal', '.notice-modal'
                    ].join(', ');
                    const style = document.createElement('style');
                    style.textContent = selectors + ' { display: none !important; visibility: hidden !important; pointer-events: none !important; opacity: 0 !important; }';
                    (document.head || document.documentElement).appendChild(style);
                })();
            "#;
            let _ = tab.evaluate(hide_ad_css, false);
            tokio::time::sleep(Duration::from_millis(500)).await;

            // 模拟按ESC键
            let _ = tab.evaluate("document.dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape', keyCode: 27}))", false);
            tokio::time::sleep(Duration::from_millis(500)).await;

            // 再次关闭弹窗
            let _ = tab.evaluate(close_popups_js, false);
            tokio::time::sleep(Duration::from_millis(500)).await;

            let _ = log_callback(String::from("正在等待页面渲染..."));

            // 等待 Vue 应用渲染
            for i in 0..40 {
                tokio::time::sleep(Duration::from_millis(200)).await;

                let has_card: bool = tab.evaluate("document.querySelector('.card-item, .longVideoCard') !== null", false)
                    .map(|r| r.value.unwrap_or_default().as_bool().unwrap_or(false))
                    .unwrap_or(false);

                let body_text_len: i64 = tab.evaluate("document.body.innerText.length", false)
                    .map(|r| r.value.unwrap_or_default().as_i64().unwrap_or(0))
                    .unwrap_or(0);

                if has_card && body_text_len > 1000 {
                    let _ = log_callback(format!("检测到视频卡片 (等待 {}ms)", i * 200));
                    break;
                }

                if i % 10 == 0 {
                    let _ = log_callback(format!("等待中... body长度: {}", body_text_len));
                }
            }

            // 再次关闭弹窗并注入CSS
            let _ = tab.evaluate(close_popups_js, false);
            tokio::time::sleep(Duration::from_millis(300)).await;
            let _ = tab.evaluate(hide_ad_css, false);

            tokio::time::sleep(Duration::from_secs(1)).await;

            // 收集所有视频
            let mut all_videos: Vec<VideoInfo> = Vec::new();
            let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut scroll_count = 0;
            let max_scrolls = 100; // 最多滚动100次

            loop {
                // 获取当前页面HTML
                let html: String = match tab.evaluate("document.documentElement.outerHTML", false) {
                    Ok(result) => result.value.unwrap_or_default().as_str().unwrap_or("").to_string(),
                    Err(_) => String::new(),
                };

                // 提取视频
                let videos = extract_videos_from_html(&html);

                // 添加新视频（去重）
                let mut new_count = 0;
                for video in videos {
                    if !seen_ids.contains(&video.id) {
                        seen_ids.insert(video.id.clone());
                        all_videos.push(video.clone());
                        new_count += 1;
                    }
                }

                if new_count > 0 {
                    let _ = log_callback(format!("第 {} 次滚动，新增 {} 个视频，累计 {} 个", scroll_count, new_count, all_videos.len()));
                } else {
                    let _ = log_callback(format!("第 {} 次滚动，无新增视频", scroll_count));
                }

                // 检查是否达到最大滚动次数
                if scroll_count >= max_scrolls {
                    let _ = log_callback(format!("达到最大滚动次数 {}，停止爬取", max_scrolls));
                    break;
                }

                // 滚动页面加载更多
                scroll_count += 1;

                // 滚动到页面底部
                let _ = tab.evaluate("window.scrollTo(0, document.body.scrollHeight)", false);

                // 等待新内容加载
                tokio::time::sleep(Duration::from_secs(2)).await;

                // 再次注入CSS隐藏新出现的弹窗
                let _ = tab.evaluate(hide_ad_css, false);

                // 短暂延迟
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            // 关闭浏览器
            let _ = tab.close(true);
            drop(tab);
            drop(browser);

            tracing::info!("[DEBUG] Total videos collected: {}", all_videos.len());

            if all_videos.is_empty() {
                return vec![ScrapeResult {
                    success: false,
                    name: "未找到视频".to_string(),
                    m3u8_url: String::new(),
                    message: "页面中未找到视频卡片".to_string(),
                    video_id: None,
                    view_count: None,
                    favorite_count: None,
                    cover_url: None,
                }];
            }

            // 转换为 ScrapeResult
            let results: Vec<ScrapeResult> = all_videos.into_iter().map(|video| {
                let views_str = video.view_count.map(|v| format!("{}", v)).unwrap_or_default();
                ScrapeResult {
                    success: true,
                    name: video.name.clone(),
                    m3u8_url: video._m3u8_url.clone(),
                    message: format!("播放:{} 收藏:{}", views_str, video.favorite_count),
                    video_id: Some(video.id),
                    view_count: video.view_count,
                    favorite_count: Some(video.favorite_count),
                    cover_url: None,
                }
            }).collect();

            let _ = log_callback(format!("完成: 成功爬取 {} 个视频", results.len()));

            results
        })
    }
}

/// 视频信息结构体
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct VideoInfo {
    id: String,
    name: String,
    _cover_url: String,
    _m3u8_url: String,
    _duration: String,
    favorite_count: i64,
    view_count: Option<i64>,
    _tag: String,
}
