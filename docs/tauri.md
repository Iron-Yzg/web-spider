# Tauri 项目技术详解

本文档详细介绍 Web-Spider 项目中使用的技术，帮助你从零开始理解 Rust 和 Tauri 开发。

## 目录

1. [Rust 语言基础](#1-rust-语言基础)
2. [Tauri 核心概念](#2-tauri-核心概念)
3. [前端与后端通信](#3-前端与后端通信)
4. [项目代码详解](#4-项目代码详解)
5. [常见问题和易错点](#5-常见问题和易错点)
6. [调试技巧](#6-调试技巧)

---

## 1. Rust 语言基础

### 1.1 变量与不可变性

Rust 的变量默认是不可变的，这是它保证内存安全的关键机制之一。

```rust
// 不可变变量（默认）
let x = 5;
// x = 6; // 错误！不能修改不可变变量

// 可变变量（需要 explicit mut）
let mut y = 5;
y = 6; // 正确

// 变量遮蔽（shadowing）
let z = 5;
let z = z + 1; // 创建新变量，可以改变类型
let z = "hello"; // 正确，类型可以改变
```

**在项目中的应用：**
```rust
// src-tauri/src/services/downloader.rs
let ffmpeg_path = find_ffmpeg();  // 不可变
// ffmpeg_path = "/new/path"; // 错误

let mut output_dir = PathBuf::from(output_path);  // 可变
fs::create_dir_all(&output_dir);  // 需要 &mut，所以要声明 mut
```

### 1.2 数据类型

Rust 分为标量类型和复合类型：

```rust
// 标量类型
let a: i32 = 42;          // 有符号整数
let b: u32 = 100;         // 无符号整数
let c: f64 = 3.14;        // 浮点数
let d: bool = true;       // 布尔值
let e: char = 'A';        // 字符（Unicode）

// 复合类型
let tuple: (i32, f64, bool) = (42, 3.14, true);
// 访问元素
let first = tuple.0;

let arr: [i32; 5] = [1, 2, 3, 4, 5];
// 访问元素
let third = arr[2];
```

**在项目中的应用：**
```rust
// src-tauri/src/commands/mod.rs
let config = state.config.lock().unwrap().clone();  // Arc<Mutex<AppConfig>>
let download_path = config.download_path;  // String

// src-tauri/src/services/downloader.rs
let output = Command::new(&ffmpeg_path)  // &String
    .args(&[...])  // &[&str]
    .output()
    .await;
```

### 1.3 所有权系统（Ownership）

Rust 最独特的特性，每个值都有一个所有者，当所有者离开作用域时，值会被自动释放。

```rust
// 所有权规则：
// 1. 每个值有一个所有者
// 2. 当所有者离开作用域，值被释放
// 3. 只有一个所有者

fn take_ownership(s: String) {
    println!("{}", s);
} // s 在这里被释放

fn main() {
    let s = String::from("hello");
    take_ownership(s);  // s 的所有权被移动到函数中
    // println!("{}", s); // 错误！s 已经不再有效
}

// 借用（ Borrowing）
fn calculate_length(s: &String) -> usize {
    s.len()
} // s 没有被释放，因为只是借用

fn main() {
    let s = String::from("hello");
    let len = calculate_length(&s);  // 借用
    println!("Length: {}", len);  // 仍然可以使用 s
}
```

**在项目中的应用：**
```rust
// src-tauri/src/services/downloader.rs
// 错误示例：忘记所有权规则
fn bad_example(video_id: String) {
    let id = video_id;  // 所有权移动
    // video_id 在这里不再可用
}

// 正确示例：使用引用
fn good_example(video_id: &str) {
    let id = video_id;  // 借用，可以继续使用
    println!("{}", id);
}

// 项目中的实际用法
pub async fn download_m3u8(
    m3u8_url: &str,        // 借用字符串切片
    output_path: &str,     // 借用
    video_id: &str,        // 借用
    video_name: &str,      // 借用
    mut progress_callback: impl FnMut(DownloadProgress),
) -> Result<(), String> {
    // 函数内部使用引用，不获取所有权
}
```

### 1.4 泛型（Generics）

泛型允许编写可以处理多种类型的代码，而不需要为每种类型重复编写。

```rust
// 简单的泛型函数
fn largest<T: PartialOrd>(list: &[T]) -> &T {
    let mut largest = &list[0];
    for item in list {
        if item > largest {
            largest = item;
        }
    }
    largest
}

// 泛型结构体
struct Point<T> {
    x: T,
    y: T,
}

// 为特定类型实现方法
impl Point<f32> {
    fn distance_from_origin(&self) -> f32 {
        (self.x.powi(2) + self.y.powi(2)).sqrt()
    }
}
```

**在项目中的应用：**
```rust
// src-tauri/src/services/mod.rs
// AppState 使用 Mutex 保护泛型类型
pub struct AppState {
    pub config: Mutex<AppConfig>,          // Mutex 是泛型
    pub videos: Mutex<Vec<VideoItem>>,     // Vec 是泛型
    pub data_dir: PathBuf,
}

// src-tauri/src/services/downloader.rs
// Result 是泛型枚举：Result<T, E>
pub async fn download_m3u8(...) -> Result<(), String> {
    // Ok(T) 或 Err(E)
    if !check_ffmpeg() {
        return Err("未找到ffmpeg".to_string());
    }
    Ok(())
}

// Option 是泛型枚举：Option<T>
fn find_video<'a>(videos: &'a Vec<VideoItem>, id: &str) -> Option<&'a VideoItem> {
    videos.iter().find(|v| v.id == id)
}
```

### 1.5 枚举与模式匹配

枚举是 Rust 中强大的类型系统，可以表示多种可能的状态。

```rust
// 简单枚举
enum VideoStatus {
    Pending,      // 待爬取
    Scraped,      // 已爬取
    Downloading,  // 下载中
    Downloaded,   // 已下载
    Failed,       // 失败
}

// 带数据的枚举
enum Message {
    Quit,                   // 没有数据
    Move { x: i32, y: i32 }, // 匿名结构体
    Write(String),          // 字符串
    ChangeColor(i32, i32, i32), // 元组
}

// match 是穷尽式模式匹配
fn status_name(status: VideoStatus) -> &'static str {
    match status {
        VideoStatus::Pending => "待爬取",
        VideoStatus::Scraped => "已爬取",
        VideoStatus::Downloading => "下载中",
        VideoStatus::Downloaded => "已下载",
        VideoStatus::Failed => "失败",
    }
}

// if let 简化单模式匹配
fn handle_status(status: VideoStatus) {
    if let VideoStatus::Downloading = status {
        println!("正在下载...");
    }
}
```

**在项目中的应用：**
```rust
// src-tauri/src/models/mod.rs
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum VideoStatus {
    Pending = 'Pending',
    Scraped = 'Scraped',
    Downloading = 'Downloading',
    Downloaded = 'Downloaded',
    Failed = 'Failed',
}

// 在 commands/mod.rs 中使用
use crate::models::VideoStatus;

fn update_video_status(videos: &mut Vec<VideoItem>, id: &str, result: Result<(), String>) {
    if let Some(video) = videos.iter_mut().find(|v| v.id == id) {
        match result {
            Ok(_) => {
                video.status = VideoStatus::Downloaded;
            }
            Err(_) => {
                video.status = VideoStatus::Scraped;
            }
        }
    }
}

// TypeScript 端的对应定义
// src/types.ts
export enum VideoStatus {
  Pending = 'Pending',
  Scraped = 'Scraped',
  Downloading = 'Downloading',
  Downloaded = 'Downloaded',
  Failed = 'Failed',
}
```

### 1.6 集合类型

Rust 标准库提供了多种集合类型：

```rust
// Vec - 动态数组
let mut vec = Vec::new();
vec.push(1);
vec.push(2);
let second = vec[1];
let first = vec.pop();

// HashMap - 键值对
use std::collections::HashMap;
let mut map = HashMap::new();
map.insert("key", "value");
if let Some(value) = map.get("key") {
    println!("{}", value);
}
```

**在项目中的应用：**
```rust
// src-tauri/src/commands/mod.rs
// Vec 用于存储视频列表
let videos_to_download: Vec<(String, String, String, PathBuf)> = video_ids
    .into_iter()  // 消耗迭代器，获取所有权
    .filter_map(|id| {
        videos_guard.iter().find(|v| v.id == id).map(|video| {
            (video.id.clone(), video.name.clone(), video.m3u8_url.clone(), PathBuf::from(&download_path))
        })
    })
    .collect();

// src/components/ScraperPage.vue
// TypeScript 中的数组
const videos = ref<VideoItem[]>([])
const downloadProgress = ref<Record<string, DownloadProgress>>({})
```

### 1.7 迭代器

迭代器提供了一种遍历集合的方式，支持惰性求值。

```rust
// 创建迭代器
let vec = vec![1, 2, 3, 4, 5];
let iter = vec.iter();

// 消费迭代器
let sum: i32 = iter.sum();

// 转换迭代器
let squares: Vec<i32> = vec.iter().map(|x| x * 2).collect();

// 过滤迭代器
let evens: Vec<&i32> = vec.iter().filter(|x| *x % 2 == 0).collect();

// 使用迭代器适配器
let result: i32 = vec
    .iter()
    .map(|x| x * 2)
    .filter(|x| *x > 5)
    .sum();
```

**在项目中的应用：**
```rust
// src-tauri/src/services/downloader.rs
// 使用 filter_map 转换和过滤
stream::iter(videos.into_iter().map(|(id, name, m3u8_url, output_dir)| {
    async move {
        // ...
        (name_clone, result)
    }
}))
.buffer_unordered(max_concurrent)
.collect()
.await;

// src-tauri/src/commands/mod.rs
// 使用 iter() 进行遍历
for id in videos_to_download.iter().map(|(id, _, _, _)| id) {
    if let Some(v) = videos_guard.iter_mut().find(|v| v.id == *id) {
        v.status = VideoStatus::Downloading;
    }
}
```

### 1.8 闭包（Closures）

闭包是可以捕获其环境的匿名函数。

```rust
// 基本语法
let add = |a, b| a + b;
let result = add(1, 2); // 3

// 捕获环境变量
let x = 5;
let closure = || println!("x = {}", x);
closure(); // 可以访问 x

// 闭包捕获方式
// Fn - 不可变借用
// FnMut - 可变借用
// FnOnce - 获取所有权（只能调用一次）
```

**在项目中的应用：**
```rust
// src-tauri/src/services/downloader.rs
// 进度回调闭包
let progress_callback = move |p: DownloadProgress| {
    let _ = sender_for_callback.send(p);
    // move 关键字将所有权移动到闭包中
};

// src-tauri/src/commands/mod.rs
// 异步闭包
task::spawn(async move {
    while let Ok(progress) = progress_rx.recv().await {
        let _ = window_clone.emit("download-progress", progress);
    }
});

// src/components/ScraperPage.vue
// Vue 中的回调函数
const handleDownload = (video: VideoItem) => {
    console.log('Downloading:', video.name)
}
```

### 1.9 生命周期（Lifetime）

生命周期确保引用始终有效，防止悬垂引用。

```rust
// 生命周期注解
fn longest<'a>(s1: &'a str, s2: &'a str) -> &'a str {
    if s1.len() > s2.len() {
        s1
    } else {
        s2
    }
}
// 'a 表示返回引用的生命周期与两个输入引用中较短的那个相同

// 结构体中的生命周期
struct User<'a> {
    name: &'a str,
}
```

**在项目中的应用：**
```rust
// src-tauri/src/commands/mod.rs
// Tauri 的 State 有隐式生命周期
#[tauri::command]
pub async fn get_videos(state: State<'_, AppState>) -> Vec<VideoItem> {
    // 'static 不是必须的，因为 State 有自己的生命周期
    state.videos.lock().unwrap().clone()
}

// 避免生命周期问题的技巧：克隆所有权
let video = {
    let videos_guard = state.videos.lock().unwrap();
    // 使用 clone() 而不是返回引用
    videos_guard[video_idx].clone()
};
```

---

## 2. Tauri 核心概念

### 2.1 Tauri 架构概述

Tauri 应用程序由两部分组成：

```
┌─────────────────────────────────────┐
│            Frontend (Vue)           │
│  ┌─────────────────────────────────┐│
│  │  HTML + CSS + JavaScript        ││
│  │  @tauri-apps/api                ││
│  └─────────────────────────────────┘│
└──────────────┬──────────────────────┘
               │ invoke()
               ▼
┌─────────────────────────────────────┐
│            Backend (Rust)           │
│  ┌─────────────────────────────────┐│
│  │  Tauri Runtime                  ││
│  │  Commands + Events              ││
│  └─────────────────────────────────┘│
│  ┌─────────────────────────────────┐│
│  │  System Layer                   ││
│  │  FileSystem, Window, etc.       ││
│  └─────────────────────────────────┘│
└─────────────────────────────────────┘
```

### 2.2 Tauri 命令（Commands）

命令是前端可以调用的 Rust 函数，使用 `#[tauri::command]` 宏标记。

```rust
// src-tauri/src/commands/mod.rs

use tauri::{State, WebviewWindow};
use crate::services::AppState;

// 基本命令
#[tauri::command]
pub fn get_videos(state: State<'_, AppState>) -> Vec<VideoItem> {
    state.videos.lock().unwrap().clone()
}

// 异步命令（重要！）
#[tauri::command]
pub async fn scrape_video(
    window: WebviewWindow,
    state: State<'_, AppState>,
    video_id: String,
) -> Result<ScrapeResult, String> {
    // 异步操作
    let result = some_async_operation().await;
    result
}

// 命令可以发出事件
#[tauri::command]
pub fn download_video(
    window: WebviewWindow,
    video_id: String,
) {
    // 发出事件到前端
    let _ = window.emit("download-progress", /* data */);
}
```

### 2.3 注册命令

命令需要在 Tauri 应用中注册：

```rust
// src-tauri/src/lib.rs

#[tauri::mobile_entry_point]
fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            // 注册所有命令
            get_config,
            update_config,
            get_videos,
            scrape_video,
            download_video,
            batch_download,
            delete_video,
            clear_downloaded,
            check_ffmpeg,
        ])
        .manage(AppState::new())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### 2.4 事件系统（Events）

事件允许后端向前端发送实时数据。

```rust
// Rust 端发出事件
#[tauri::command]
pub async fn download_video(
    window: WebviewWindow,
    state: State<'_, AppState>,
    video_id: String,
) {
    // 发送进度事件
    let progress = DownloadProgress {
        video_id: video_id.clone(),
        progress: 50,
        status: "正在下载...".to_string(),
        speed: "10 MB/s".to_string(),
        eta: "00:30".to_string(),
    };
    let _ = window.emit("download-progress", progress);

    // 发送视频列表更新事件
    let _ = window.emit("videos-updated", state.videos.lock().unwrap().clone());
}

// Rust 端监听事件（在异步任务中）
let window_clone = window.clone();
task::spawn(async move {
    // 监听进度
    while let Ok(progress) = progress_rx.recv().await {
        let _ = window_clone.emit("download-progress", progress);
    }
});
```

### 2.5 应用状态管理（AppState）

使用 `State` 管理全局状态：

```rust
// src-tauri/src/services/mod.rs

use std::sync::Mutex;
use crate::models::{AppConfig, VideoItem};

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub videos: Mutex<Vec<VideoItem>>,
    pub data_dir: PathBuf,
}

impl AppState {
    pub fn new() -> Self {
        let data_dir = dirs::data_dir()
            .unwrap_or(PathBuf::from("./data"))
            .join("web-spider");
        let _ = fs::create_dir_all(&data_dir);

        let config = Self::load_config(&data_dir);
        let videos = Self::load_videos(&data_dir);

        Self {
            config: Mutex::new(config),
            videos: Mutex::new(videos),
            data_dir,
        }
    }

    pub fn save_videos(&self) {
        let videos = self.videos.lock().unwrap();
        let path = self.data_dir.join("videos.json");
        if let Ok(content) = serde_json::to_string_pretty(&*videos) {
            let _ = fs::write(path, content);
        }
    }
}

// 在命令中使用 State
#[tauri::command]
pub fn get_videos(state: State<'_, AppState>) -> Vec<VideoItem> {
    state.videos.lock().unwrap().clone()
}
```

---

## 3. 前端与后端通信

### 3.1 调用命令（Invoke）

前端通过 `invoke` 函数调用 Rust 命令：

```typescript
// src/components/ScraperPage.vue

import { invoke } from '@tauri-apps/api/core'

// 调用无参数命令
const videos = await invoke<VideoItem[]>('get_videos')

// 调用有参数命令
await invoke('delete_video', { videoId: id })

// 调用异步命令（自动等待结果）
const result = await invoke<ScrapeResult>('scrape_video', {
    videoId: videoId.value.trim()
})

// 调用返回 Result 的命令
try {
    await invoke('download_video', { videoId: video.id })
} catch (e) {
    console.error('下载失败:', e)
}
```

### 3.2 监听事件（Listen）

前端通过 `listen` 函数监听 Rust 发出的事件：

```typescript
// src/components/ScraperPage.vue

import { listen } from '@tauri-apps/api/event'

onMounted(async () => {
    // 监听视频列表更新
    const unlistenVideos = await listen<VideoItem[]>('videos-updated', (event) => {
        videos.value = event.payload
        console.log('视频列表已更新:', event.payload.length, '个视频')
    })

    // 监听下载进度
    const unlistenProgress = await listen<DownloadProgress>('download-progress', (event) => {
        const progress = event.payload
        downloadProgress.value[progress.video_id] = progress
        console.log(`下载进度: ${progress.progress}% - ${progress.status}`)
    })

    // 监听爬取日志
    const unlistenScrapeLog = await listen<string>('scrape-log', (event) => {
        console.log('爬取日志:', event.payload)
    })

    // 清理监听器
    onUnmounted(() => {
        unlistenVideos()
        unlistenProgress()
        unlistenScrapeLog()
    })
})
```

### 3.3 类型同步

前后端类型需要保持一致：

```typescript
// src/types.ts
export enum VideoStatus {
  Pending = 'Pending',
  Scraped = 'Scraped',
  Downloading = 'Downloading',
  Downloaded = 'Downloaded',
  Failed = 'Failed',
}

export interface VideoItem {
  id: string
  name: string
  m3u8_url: string
  status: VideoStatus
  created_at: string
  downloaded_at?: string
}

export interface DownloadProgress {
  video_id: string
  progress: number
  status: string
  speed: string
  eta: string
}
```

```rust
// src-tauri/src/models/mod.rs
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum VideoStatus {
    #[serde(rename = "Pending")]
    Pending,
    #[serde(rename = "Scraped")]
    Scraped,
    #[serde(rename = "Downloading")]
    Downloading,
    #[serde(rename = "Downloaded")]
    Downloaded,
    #[serde(rename = "Failed")]
    Failed,
}

// Serialize 派生宏自动生成序列化代码
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DownloadProgress {
    #[serde(rename = "video_id")]
    pub video_id: String,
    pub progress: u8,
    pub status: String,
    pub speed: String,
    pub eta: String,
}
```

---

## 4. 项目代码详解

### 4.1 爬虫服务（scraper.rs）

使用 headless_chrome 进行网页爬取：

```rust
// src-tauri/src/services/scraper.rs

use headless_chrome::{Browser, LaunchOptions};

pub async fn scrape_m3u8(
    video_id: &str,
    local_storage_json: &str,
    log_callback: impl Fn(String),
) -> Result<ScrapeResult, String> {
    // 1. 启动浏览器
    let browser = Browser::new(LaunchOptions {
        headless: true,
        ..Default::default()
    }).map_err(|e| format!("启动浏览器失败: {}", e))?;

    let tab = browser.new_tab().map_err(|e| format!("创建标签页失败: {}", e))?;

    // 2. 导航到目标页面
    let url = format!("https://example.com/video/{}", video_id);
    tab.navigate_to(&url).map_err(|e| format!("导航失败: {}", e))?;

    // 3. 等待页面加载
    tab.wait_for_elements("div.video-title").map_err(|e| format!("等待元素失败: {}", e))?;

    // 4. 注入 localStorage（认证信息）
    let local_storage: Vec<LocalStorageItem> = serde_json::from_str(local_storage_json)
        .unwrap_or_default();
    for item in local_storage {
        tab.evaluate(&format!(
            "localStorage.setItem('{}', '{}')",
            item.key, item.value
        )).await?;
    }

    // 5. 提取数据
    let name: String = tab.evaluate("document.querySelector('div.video-title').textContent.trim()")
        .await?
        .result
        .as_str()
        .ok_or("无法获取标题")?
        .to_string();

    // 6. 获取 m3u8 URL（通过网络请求拦截）
    // 这里需要使用更复杂的方法获取
}
```

### 4.2 下载服务（downloader.rs）

使用 FFmpeg 下载和转码视频：

```rust
// src-tauri/src/services/downloader.rs

use tokio::process::Command;
use std::process::Stdio;

// 下载 M3U8 视频
pub async fn download_m3u8(
    m3u8_url: &str,
    output_path: &str,
    video_id: &str,
    video_name: &str,
    mut progress_callback: impl FnMut(DownloadProgress),
) -> Result<(), String> {
    // 1. 检查 FFmpeg
    if !check_ffmpeg() {
        return Err("未找到ffmpeg".to_string());
    }

    // 2. 下载 m3u8 文件
    progress_callback(DownloadProgress {
        video_id: video_id.to_string(),
        progress: 5,
        status: "正在下载m3u8文件...".to_string(),
        speed: "0 MB/s".to_string(),
        eta: "--:--".to_string(),
    });

    let client = reqwest::Client::new();
    let m3u8_content = client.get(m3u8_url)
        .send().await?
        .text().await?;

    // 3. 调用 FFmpeg
    let ffmpeg_path = find_ffmpeg();
    let output = Command::new(&ffmpeg_path)
        .args(&[
            "-y",  // 覆盖已存在的文件
            "-protocol_whitelist", "file,http,https,tcp,tls,crypto",
            "-i", &m3u8_url,  // 输入文件
            "-c", "copy",     // 直接复制流（不重新编码）
            output_path,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    Ok(())
}

// 并发批量下载
pub async fn batch_download_concurrent(
    videos: Vec<(String, String, String, PathBuf)>,
    max_concurrent: usize,
    progress_sender: broadcast::Sender<DownloadProgress>,
) -> Vec<(String, Result<(), String>)> {
    use futures::stream::{self, StreamExt};

    stream::iter(videos.into_iter().map(|(id, name, m3u8_url, output_dir)| {
        async move {
            let result = download_m3u8(&m3u8_url, &output_dir.to_string_lossy(), &id, &name, |p| {
                let _ = progress_sender.send(p);
            }).await;

            (name, result)
        }
    }))
    .buffer_unordered(max_concurrent)  // 最多 max_concurrent 个并发
    .collect()
    .await
}
```

### 4.3 异步编程与 Tokio

项目大量使用异步编程来处理并发下载：

```rust
// Tokio 任务
use tokio::task;

// spawn 创建新任务
task::spawn(async move {
    // 异步代码
    do_something().await;
});

// 并发等待多个任务
use tokio::join;

async fn fetch_data() -> String {
    // HTTP 请求
    reqwest::get("https://api.example.com/data")
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
}

async fn concurrent_operations() {
    let (data1, data2) = join!(
        fetch_data(),  // 并发执行
        fetch_data()
    );
    println!("Data 1: {}, Data 2: {}", data1.len(), data2.len());
}

// broadcast channel 用于多消费者
use tokio::sync::broadcast;

let (tx, rx) = broadcast::channel(100);

// 发送端
let _ = tx.send(progress);

// 接收端（可以有多个）
let mut rx1 = rx.clone();
let mut rx2 = rx.clone();

task::spawn(async move {
    while let Ok(progress) = rx1.recv().await {
        println!("Receiver 1: {:?}", progress);
    }
});

task::spawn(async move {
    while let Ok(progress) = rx2.recv().await {
        println!("Receiver 2: {:?}", progress);
    }
});
```

### 4.4 错误处理

Rust 的错误处理主要使用 `Result` 和 `?` 运算符：

```rust
// 基本错误处理
fn may_fail() -> Result<String, String> {
    Ok("success".to_string())
}

fn caller() -> Result<(), String> {
    let result = may_fail()?;  // ? 运算符传播错误
    Ok(())
}

// 组合多个错误来源
fn complex_operation() -> Result<VideoItem, String> {
    // 使用 serde_json 解析
    let json_str = std::fs::read_to_string("file.json")
        .map_err(|e| format!("读取文件失败: {}", e))?;

    let video: VideoItem = serde_json::from_str(&json_str)
        .map_err(|e| format!("解析JSON失败: {}", e))?;

    Ok(video)
}

// 在 async 函数中
async fn async_operation() -> Result<String, String> {
    let response = reqwest::get("https://example.com")
        .await
        .map_err(|e| format!("网络请求失败: {}", e))?;

    let text = response.text()
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    Ok(text)
}
```

---

## 5. 常见问题和易错点

### 5.1 所有权相关错误

```rust
// 错误：所有权问题
fn bad_example(video: VideoItem) {
    let id = video.id;  // 克隆 String
    // video 在这里被释放
}

// 正确：使用引用
fn good_example(video: &VideoItem) {
    let id = &video.id;  // 借用
    // video 没有被释放
}

// 错误：在循环中移动所有权
fn bad_iteration(videos: &mut Vec<VideoItem>) {
    for video in videos {  // video 的所有权被移动
        // 处理 video
    }  // video 在这里被释放，但 videos 仍然可变
}

// 正确：使用迭代器引用
fn good_iteration(videos: &Vec<VideoItem>) {
    for video in videos.iter() {
        // 处理 video 的引用
    }
}
```

### 5.2 生命周期问题

```rust
// 错误：返回引用
fn bad_function(videos: &Vec<VideoItem>) -> &VideoItem {
    &videos[0]  // 返回引用，但生命周期不明确
}

// 正确：返回克隆的值
fn good_function(videos: &Vec<VideoItem>) -> VideoItem {
    videos[0].clone()  // 返回所有权
}

// 错误：异步中的生命周期
#[tauri::command]
pub async fn bad_command(state: State<'_, AppState>) {
    task::spawn(async move {
        let videos = state.videos.lock().unwrap();  // 错误！state 的生命周期不满足 'static
    });
}

// 正确：克隆数据后再使用
#[tauri::command]
pub async fn good_command(state: State<'_, AppState>) {
    let videos_clone = state.videos.lock().unwrap().clone();
    task::spawn(async move {
        // 使用克隆的数据
        println!("{}", videos_clone.len());
    });
}
```

### 5.3 异步错误

```rust
// 错误：忘记 await
async fn bad_async() {
    let result = some_async_function();  // 返回 Future，不执行
}

// 正确：使用 await
async fn good_async() {
    let result = some_async_function().await;  // 执行异步操作
}

// 错误：在同步函数中调用异步代码
fn sync_function() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // 异步代码
    });
}
```

### 5.4 类型不匹配

```rust
// 错误：String 和 &str
fn takes_str(s: &str) {}
fn main() {
    let string = String::from("hello");
    takes_str(string);  // 错误！需要 &str
    takes_str(&string);  // 正确
}

// 错误：借用规则
fn mutate_string(s: &mut String) {
    s.push_str(" world");
}

fn main() {
    let mut string = String::from("hello");
    mutate_string(&mut string);  // 正确
}
```

### 5.5 并发安全问题

```rust
// 错误：多个线程访问可变状态
use std::sync::Mutex;

let mut data = Mutex::new(0);
let handle1 = std::thread::spawn(|| {
    *data.lock().unwrap() += 1;
});
let handle2 = std::thread::spawn(|| {
    *data.lock().unwrap() += 1;
});
// 可能的数据竞争

// 正确：使用 Arc（原子引用计数）
use std::sync::{Arc, Mutex};

let data = Arc::new(Mutex::new(0));
let data_clone = Arc::clone(&data);

let handle1 = std::thread::spawn(move || {
    *data_clone.lock().unwrap() += 1;
});
// ...
```

---

## 6. 调试技巧

### 6.1 使用 eprintln! 输出调试信息

```rust
// 在 Rust 代码中添加调试输出
eprintln!("[DEBUG] video_id: {}", video_id);
eprintln!("[DEBUG] m3u8_url: {}", m3u8_url);
eprintln!("[DEBUG] progress: {}%", progress);
```

### 6.2 打开 Tauri 日志

```rust
// src-tauri/src/lib.rs
tauri::Builder::default()
    .plugin(tauri_plugin_log::Builder::default().build())
```

### 6.3 使用浏览器开发者工具

在 Tauri 窗口右键 -> "检查" 打开开发者工具。

### 6.4 类型检查

```bash
# Rust 类型检查
cargo check

# TypeScript 类型检查
pnpm tsc --noEmit
```

### 6.5 格式化代码

```bash
# Rust 代码格式化
rustfmt src-tauri/src/commands/mod.rs

# 格式化整个项目
cargo fmt

# Vue/TypeScript 格式化
pnpm format
```

---

## 推荐学习资源

### Rust 学习
- [Rust 官方教程](https://doc.rust-lang.org/book/)
- [Rust by Example](https://doc.rust-lang.org/rust-by-example/)
- [Rustlings](https://github.com/rust-lang/rustlings/) - 练习题

### Tauri 学习
- [Tauri 官方文档](https://tauri.app/)
- [Tauri API 示例](https://github.com/tauri-apps/tauri/tree/master/examples)

### Vue 3 学习
- [Vue 3 官方文档](https://vuejs.org/)
- [Vue Composition API](https://vuejs.org/api/composition-api-setup.html)

---

## 快速参考

### Cargo 常用命令

```bash
cargo check          # 检查编译（不生成二进制）
cargo build          # Debug 构建
cargo build --release # Release 构建
cargo run            # 运行程序
cargo test           # 运行测试
cargo clean          # 清理构建产物
cargo update         # 更新依赖
cargo doc --open     # 生成并打开文档
```

### pnpm 常用命令

```bash
pnpm install         # 安装依赖
pnpm dev             # 开发模式运行
pnpm build           # 生产构建
pnpm tauri dev       # Tauri 开发模式
pnpm tauri build     # Tauri 生产构建
```

### 关键文件速查

| 文件 | 作用 |
|------|------|
| `src-tauri/src/lib.rs` | Tauri 应用入口，注册命令 |
| `src-tauri/src/commands/mod.rs` | 命令实现 |
| `src-tauri/src/services/downloader.rs` | 下载逻辑 |
| `src-tauri/src/services/scraper.rs` | 爬虫逻辑 |
| `src-tauri/src/services/mod.rs` | 应用状态管理 |
| `src/components/ScraperPage.vue` | 主界面 |
| `src/types.ts` | TypeScript 类型定义 |
