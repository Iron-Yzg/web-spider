# Web-Spider - M3U8 视频爬取与下载工具

一个基于 Tauri 的跨平台桌面/移动应用程序，用于从网页爬取 M3U8 视频链接并下载视频。

## 功能特性

| 功能 | 桌面端 | 移动端 |
|------|--------|--------|
| 视频爬取 | ✅ | ❌ |
| 视频下载 | ✅ | ❌ |
| 视频播放 | ✅ | ✅ |
| 批量下载 | ✅ | ❌ |
| 网站管理 | ✅ | 只读 |
| 本地存储 | ✅ | ✅ |

## 技术栈

| 层级 | 技术 |
|------|------|
| 前端框架 | Vue 3 + TypeScript |
| 构建工具 | Vite 6 |
| 桌面框架 | Tauri 2 (桌面 + 移动) |
| 桌面爬虫 | headless_chrome |
| 视频处理 | FFmpeg |
| 异步运行时 | Tokio |
| HTTP 客户端 | reqwest |

## 环境要求

### 必需依赖

1. **Node.js** (>= 18)
   ```bash
   # macOS
   brew install node

   # 或使用 nvm
   nvm install 20
   nvm use 20
   ```

2. **Rust** (>= 1.70)
   ```bash
   # macOS/Linux
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

   # Windows
   # 下载 https://rustup.rs/ 并运行
   ```

3. **FFmpeg** (>= 4.0)
   ```bash
   # macOS
   brew install ffmpeg

   # Ubuntu/Debian
   sudo apt install ffmpeg

   # Windows
   # 下载 https://ffmpeg.org/download.html
   ```

4. **Tauri CLI**
   ```bash
   cargo install tauri-cli
   ```

### 系统依赖

**macOS:**
```bash
# Xcode Command Line Tools（桌面端必需）
xcode-select --install

# 对于 iOS 开发，还需要 Xcode
# 从 App Store 安装 Xcode
```

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install libwebkit2gtk-4.0-dev libappindicator3-0.1-cargo libssl-dev
```

**Windows:**
- 安装 WebView2 Runtime（Windows 11 已内置）
- 安装 C++ 构建工具（Visual Studio Build Tools）

## 安装步骤

### 1. 安装 pnpm（推荐）或 npm

```bash
npm install -g pnpm
# 或
npm install -g yarn
```

### 2. 安装项目依赖

```bash
# 进入项目根目录
cd web-spider

# 安装前端依赖
pnpm install
```

### 3. 下载 FFmpeg 资源（用于打包）

项目需要将 FFmpeg 打包到应用中：

```bash
cd src-tauri
node scripts/download-ffmpeg.cjs
```

## 开发运行

### 桌面端开发（完整功能，包含爬虫）

```bash
# 使用 pnpm
pnpm dev:desktop

# 或使用 npm
npm run dev:desktop

# 或直接使用 cargo
cargo tauri dev
```

### 移动端开发（仅视频播放功能）

**iOS:**
```bash
# 使用 pnpm
pnpm dev:ios

# 或使用 npm
npm run dev:ios

# 或直接使用 cargo
cargo tauri ios dev
```

**Android:**
```bash
# 使用 pnpm
pnpm dev:android

# 或使用 npm
npm run dev:android

# 或直接使用 cargo
cargo tauri android dev
```

### 通用移动端开发

```bash
# 同时支持 iOS 和 Android（需要同时配置两个平台）
pnpm dev:mobile
```

### 开发模式特点

- 前端代码修改后自动热更新
- Rust 代码修改后自动重新编译
- 打开开发者工具（在 Tauri 窗口右键 -> 检查）

## 生产构建

### 桌面端打包

**发布版本（推荐）:**
```bash
# 使用 pnpm
pnpm build:desktop

# 或使用 npm
npm run build:desktop
```

**调试版本（包含调试信息）:**
```bash
pnpm build:desktop:debug
```

构建产物位于：
- **macOS**: `src-tauri/target/release/bundle/dmg/` 或 `.tar.gz`
- **Windows**: `src-tauri/target/release/bundle/msi/`
- **Linux**: `src-tauri/target/release/bundle/deb/`

### iOS 打包

```bash
# 调试版本
pnpm build:ios

# 发布版本
pnpm build:ios:release

# 产物位置: src-tauri/target/release/bundle/ios/
```

**注意:** iOS 打包前需要：
1. 在 Apple Developer Program 中创建应用 ID
2. 配置代码签名证书
3. 更新 `src-tauri/capabilities/desktop.json` 中的 bundle identifier

### Android 打包

```bash
# 调试版本
pnpm build:android

# 发布版本
pnpm build:android:release

# 产物位置: src-tauri/target/release/bundle/android/
```

### macOS 签名与公证

```bash
# 证书签名
codesign --sign "Developer ID Application: Your Name" --timestamp --entitlements src-tauri/entitlements.plist src-tauri/target/release/bundle/dmg/web-spider_*.dmg

# 公证（用于 Gatekeeper）
xcrun altool --notarize-app --primary-bundle-id com.yangzhenguo.web-spider --username "your@email.com" --password "app-specific-password" --file web-spider_*.dmg
```

## 清理命令

### 清理构建产物

```bash
# 清理 Cargo 构建产物
pnpm clean

# 或使用 cargo
cargo clean

# 清理 Vite 缓存
rm -rf node_modules/.vite
```

### 完整清理（推荐在出现问题时执行）

```bash
# 清理所有构建产物和依赖
pnpm clean:all

# 手动执行
cargo clean
rm -rf node_modules
rm -rf src-tauri/target
rm -rf src-tauri/Cargo.lock

# 重新安装
pnpm install

# 重新构建
pnpm build:desktop
```

### 清理特定平台

```bash
# 仅清理 iOS 构建产物
rm -rf src-tauri/target/aarch64-apple-ios
rm -rf src-tauri/target/aarch64-apple-ios-sim

# 仅清理 Android 构建产物
rm -rf src-tauri/target/aarch64-linux-android
rm -rf src-tauri/target/armv7-linux-androideabi
```

## npm 脚本命令速查表

| 命令 | 描述 |
|------|------|
| `pnpm dev` | 仅启动 Vite 前端开发服务器 |
| `pnpm build` | 构建前端生产版本 |
| `pnpm preview` | 预览前端构建产物 |
| `pnpm lint` | 检查前端类型错误 |
| `pnpm dev:desktop` | 启动桌面端开发（完整功能） |
| `pnpm dev:mobile` | 启动移动端开发 |
| `pnpm dev:ios` | 启动 iOS 模拟器开发 |
| `pnpm dev:android` | 启动 Android 模拟器开发 |
| `pnpm build:desktop` | 打包桌面端发布版本 |
| `pnpm build:desktop:debug` | 打包桌面端调试版本 |
| `pnpm build:ios` | 打包 iOS 调试版本 |
| `pnpm build:ios:release` | 打包 iOS 发布版本 |
| `pnpm build:android` | 打包 Android 调试版本 |
| `pnpm build:android:release` | 打包 Android 发布版本 |
| `pnpm clean` | 清理构建产物 |
| `pnpm clean:all` | 完整清理（包含依赖） |

## Cargo Feature 说明

### 桌面端特性（默认启用）

```toml
[features]
desktop = ["headless_chrome"]  # 包含爬虫功能
```

### 移动端特性

```bash
# 构建时不包含 headless_chrome，减小包体积
cargo tauri ios dev --features mobile
```

| Feature | 桌面端 | 移动端 |
|---------|--------|--------|
| headless_chrome | ✅ | ❌ |
| 爬虫功能 | ✅ | ❌ |
| 下载功能 | ✅ | ❌ |
| 视频播放 | ✅ | ✅ |

## 常见问题

### Q: 运行时提示 "未找到 FFmpeg"

A: 确保系统已安装 FFmpeg，或运行：
```bash
cd src-tauri
node scripts/download-ffmpeg.cjs
```

### Q: macOS 构建失败，提示权限问题

A: 在终端执行：
```bash
sudo xcode-select --reset
```

### Q: Windows 打包失败，提示缺少 WebView2

A: 安装 WebView2 Runtime：https://developer.microsoft.com/en-us/microsoft-edge/webview2/

### Q: iOS 模拟器无法启动

A: 检查 Xcode 版本和模拟器是否已安装：
```bash
# 查看可用的模拟器
xcrun simctl list devices available

# 启动特定模拟器
xcrun simctl boot "iPhone 15"
```

### Q: 依赖安装缓慢或失败

A: 使用国内镜像源：
```bash
# Rust 镜像
export RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
export RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup

# npm/pnpm 镜像
pnpm config set registry https://registry.npmmirror.com
```

### Q: Android 开发环境配置

A: 确保已安装 Android Studio 和 SDK：
```bash
# 检查 Android SDK
echo $ANDROID_HOME
echo $ANDROID_SDK_ROOT

# 如未设置，添加到 ~/.zshrc
export ANDROID_HOME=$HOME/Library/Android/sdk
export ANDROID_SDK_ROOT=$ANDROID_HOME
```

## 项目结构

```
web-spider/
├── src/                          # Vue 前端源码
│   ├── main.ts                   # Vue 入口
│   ├── App.vue                   # 根组件
│   ├── types.ts                  # TypeScript 类型定义
│   ├── components/
│   │   ├── ScraperPage.vue       # 主爬取界面（桌面端）
│   │   ├── ConfigPage.vue        # 设置页面（桌面端）
│   │   ├── VideoPlayer.vue       # 视频播放器
│   │   └── LogPopup.vue          # 日志弹窗（桌面端）
│   └── views/
│       └── HomePage.vue          # 首页
├── src-tauri/                    # Tauri 后端源码
│   ├── src/
│   │   ├── lib.rs                # Tauri 入口
│   │   ├── main.rs               # 应用入口
│   │   ├── commands/             # 命令处理
│   │   │   └── mod.rs
│   │   ├── models/               # 数据模型
│   │   │   └── mod.rs
│   │   ├── services/             # 业务逻辑（仅桌面端）
│   │   │   ├── mod.rs
│   │   │   ├── scraper/          # 爬虫模块
│   │   │   │   ├── srl_spider.rs
│   │   │   │   └── mod.rs
│   │   │   └── downloader.rs     # 下载服务
│   │   └── db/                   # 数据库
│   │       └── mod.rs
│   ├── tauri.conf.json           # Tauri 配置
│   ├── Cargo.toml                # Rust 依赖配置
│   ├── capabilities/             # 权限配置
│   │   ├── desktop.json
│   │   └── mobile.json
│   └── scripts/
│       └── download-ffmpeg.cjs   # FFmpeg 下载脚本
├── android/                      # Android 原生代码
│   ├── src/main/
│   │   ├── kotlin/
│   │   │   └── com/
│   │   │       └── yangzhenguo/
│   │   │           └── webspider/
│   │   │               └── MainActivity.kt
│   │   └── AndroidManifest.xml
├── ios/                          # iOS 原生代码
│   ├── Sources/
│   │   └── WebSpider/
│   │       └── WebSpiderApp.swift
│   └── Resources/
│       └── Assets.xcassets
├── package.json                  # Node 依赖配置
├── pnpm-lock.yaml
└── vite.config.ts                # Vite 配置

```

## License

MIT License
