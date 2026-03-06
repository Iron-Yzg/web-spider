use headless_chrome::Browser;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;

/// 嗅探到的媒体资源
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SniffedMedia {
    /// 媒体资源URL
    pub url: String,
    /// 媒体类型: video, audio, hls, dash, stream
    pub media_type: String,
    /// 文件扩展名: mp4, m3u8, flv, ts, mp3 等
    pub file_ext: String,
    /// 文件大小(字节), 来自 performance entry
    pub size: Option<u64>,
    /// 资源来源: dom(页面元素), network(网络请求), script(脚本内嵌)
    pub source: String,
}

/// 嗅探结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SniffResult {
    /// 页面URL
    pub page_url: String,
    /// 页面标题
    pub page_title: String,
    /// 检测到的媒体列表
    pub media_list: Vec<SniffedMedia>,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 嗅探指定页面中的媒体资源
pub fn sniff_page(url: &str, timeout_secs: u64, log_callback: impl Fn(String)) -> SniffResult {
    log_callback(format!("开始嗅探: {}", url));

    // 启动浏览器
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

    let browser = match Browser::new(headless_chrome::LaunchOptions {
        args: browser_args,
        headless: false, // headless=new via args
        ..Default::default()
    }) {
        Ok(b) => b,
        Err(e) => {
            return SniffResult {
                page_url: url.to_string(),
                page_title: String::new(),
                media_list: vec![],
                success: false,
                message: format!("启动浏览器失败: {}", e),
            };
        }
    };

    let tab = match browser.new_tab() {
        Ok(t) => t,
        Err(e) => {
            return SniffResult {
                page_url: url.to_string(),
                page_title: String::new(),
                media_list: vec![],
                success: false,
                message: format!("创建标签页失败: {}", e),
            };
        }
    };

    log_callback("正在加载页面...".to_string());

    // 启用网络事件拦截 (在导航之前)
    let _ = tab.call_method(headless_chrome::protocol::cdp::Network::Enable {
        max_total_buffer_size: None,
        max_resource_buffer_size: None,
        max_post_data_size: None,
        enable_durable_messages: None,
        report_direct_socket_traffic: None,
    });

    // 导航到页面
    if let Err(e) = tab.navigate_to(url) {
        return SniffResult {
            page_url: url.to_string(),
            page_title: String::new(),
            media_list: vec![],
            success: false,
            message: format!("导航失败: {}", e),
        };
    }

    // 等待页面加载
    let wait_duration = std::time::Duration::from_secs(timeout_secs.min(30));
    let _ = tab.wait_until_navigated();
    std::thread::sleep(wait_duration);

    log_callback("页面加载完成，正在分析媒体资源...".to_string());

    // 获取页面标题
    let page_title = tab
        .evaluate("document.title", false)
        .ok()
        .and_then(|v| v.value)
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    // 执行 JavaScript 收集所有媒体资源
    let js_code = r#"
(() => {
    const mediaExts = /\.(m3u8|mp4|flv|ts|webm|mkv|avi|mov|mpd|m4s|mp3|m4a|aac|wav|ogg|flac|wma)(\?|#|$)/i;
    const hlsPattern = /\.m3u8(\?|#|$)/i;
    const dashPattern = /\.mpd(\?|#|$)/i;
    const videoExts = /\.(mp4|flv|webm|mkv|avi|mov|ts|m4s)(\?|#|$)/i;
    const audioExts = /\.(mp3|m4a|aac|wav|ogg|flac|wma)(\?|#|$)/i;
    
    const results = [];
    const seen = new Set();
    
    function addResult(url, source) {
        if (!url || seen.has(url) || url.startsWith('data:') || url.startsWith('blob:')) return;
        // 过滤掉小图片和非媒体资源
        if (url.match(/\.(png|jpg|jpeg|gif|svg|ico|css|js|woff|ttf|eot)(\?|#|$)/i)) return;
        if (!mediaExts.test(url) && !url.includes('.m3u8') && !url.includes('.mpd')) return;
        seen.add(url);
        
        let mediaType = 'video';
        let fileExt = '';
        
        if (hlsPattern.test(url)) {
            mediaType = 'hls';
            fileExt = 'm3u8';
        } else if (dashPattern.test(url)) {
            mediaType = 'dash';
            fileExt = 'mpd';
        } else if (audioExts.test(url)) {
            mediaType = 'audio';
            const match = url.match(/\.(\w+)(\?|#|$)/);
            fileExt = match ? match[1].toLowerCase() : 'audio';
        } else if (videoExts.test(url)) {
            mediaType = 'video';
            const match = url.match(/\.(\w+)(\?|#|$)/);
            fileExt = match ? match[1].toLowerCase() : 'video';
        } else {
            const match = url.match(/\.(\w+)(\?|#|$)/);
            fileExt = match ? match[1].toLowerCase() : 'unknown';
        }
        
        results.push({ url, mediaType, fileExt, source, size: null });
    }
    
    // 1. Performance API - 捕获所有网络加载的资源
    try {
        performance.getEntriesByType('resource').forEach(entry => {
            if (mediaExts.test(entry.name)) {
                const item = { url: entry.name, source: 'network' };
                // 获取文件大小
                const size = entry.transferSize || entry.decodedBodySize || null;
                addResult(entry.name, 'network');
                // 如果有大小信息，更新最后添加的结果
                if (size && results.length > 0 && results[results.length - 1].url === entry.name) {
                    results[results.length - 1].size = size;
                }
            }
        });
    } catch(e) {}
    
    // 2. DOM 元素扫描
    try {
        document.querySelectorAll('video, audio').forEach(el => {
            // 直接 src
            if (el.src) addResult(el.src, 'dom');
            if (el.currentSrc) addResult(el.currentSrc, 'dom');
            // data-src 等属性
            ['data-src', 'data-url', 'data-video-url', 'data-source'].forEach(attr => {
                const val = el.getAttribute(attr);
                if (val && (val.startsWith('http') || val.startsWith('//'))) {
                    addResult(val.startsWith('//') ? 'https:' + val : val, 'dom');
                }
            });
        });
        
        // source 元素
        document.querySelectorAll('source').forEach(el => {
            if (el.src) addResult(el.src, 'dom');
            const dataSrc = el.getAttribute('data-src');
            if (dataSrc) addResult(dataSrc.startsWith('//') ? 'https:' + dataSrc : dataSrc, 'dom');
        });
        
        // embed / object
        document.querySelectorAll('embed, object').forEach(el => {
            const src = el.src || el.data || el.getAttribute('data');
            if (src && mediaExts.test(src)) addResult(src, 'dom');
        });
    } catch(e) {}
    
    // 3. iframe 中可能的视频播放器
    try {
        document.querySelectorAll('iframe').forEach(el => {
            const src = el.src || el.getAttribute('data-src') || '';
            if (src && (
                src.includes('player') || src.includes('video') || 
                src.includes('embed') || src.includes('.m3u8') ||
                src.includes('.mp4') || src.includes('play')
            )) {
                addResult(src, 'iframe');
            }
        });
    } catch(e) {}
    
    // 4. 扫描页面脚本中的视频URL
    try {
        const scripts = document.querySelectorAll('script:not([src])');
        const urlRegex = /(?:https?:)?\/\/[^\s"'<>]+\.(?:m3u8|mp4|flv|mpd)(?:\?[^\s"'<>]*)?/gi;
        scripts.forEach(script => {
            const content = script.textContent || '';
            const matches = content.match(urlRegex);
            if (matches) {
                matches.forEach(m => {
                    const url = m.startsWith('//') ? 'https:' + m : m;
                    addResult(url, 'script');
                });
            }
        });
    } catch(e) {}
    
    // 5. 检查常见播放器的全局变量
    try {
        // videojs
        if (window.videojs) {
            document.querySelectorAll('.video-js').forEach(el => {
                const player = window.videojs(el.id);
                if (player && player.currentSrc) {
                    addResult(player.currentSrc(), 'player');
                }
            });
        }
        // jwplayer
        if (window.jwplayer) {
            try {
                const p = window.jwplayer();
                if (p && p.getPlaylistItem) {
                    const item = p.getPlaylistItem();
                    if (item && item.file) addResult(item.file, 'player');
                }
            } catch(e) {}
        }
        // DPlayer
        if (window.dp && window.dp.video) {
            addResult(window.dp.video.src, 'player');
        }
    } catch(e) {}
    
    return JSON.stringify(results);
})()
"#;

    let media_list = match tab.evaluate(js_code, false) {
        Ok(result) => {
            if let Some(value) = result.value {
                let json_str = value.as_str().unwrap_or("[]");
                match serde_json::from_str::<Vec<RawSniffedMedia>>(json_str) {
                    Ok(raw_list) => {
                        raw_list
                            .into_iter()
                            .map(|r| SniffedMedia {
                                url: r.url,
                                media_type: r.media_type,
                                file_ext: r.file_ext,
                                size: r.size.and_then(|s| if s > 0 { Some(s) } else { None }),
                                source: r.source,
                            })
                            .collect()
                    }
                    Err(e) => {
                        log_callback(format!("解析嗅探结果失败: {}", e));
                        vec![]
                    }
                }
            } else {
                vec![]
            }
        }
        Err(e) => {
            log_callback(format!("执行嗅探脚本失败: {}", e));
            vec![]
        }
    };

    let count = media_list.len();
    log_callback(format!("嗅探完成，发现 {} 个媒体资源", count));

    SniffResult {
        page_url: url.to_string(),
        page_title,
        media_list,
        success: true,
        message: format!("发现 {} 个媒体资源", count),
    }
}

/// 内部用于 JSON 反序列化
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSniffedMedia {
    url: String,
    media_type: String,
    file_ext: String,
    size: Option<u64>,
    source: String,
}
