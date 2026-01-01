# Web-Spider - M3U8 视频爬取与下载工具

一个基于 Tauri 的桌面应用程序，用于从网页爬取 M3U8 视频链接并下载视频。

## 功能特性

- 🎯 **视频爬取** - 输入视频 ID，自动爬取 M3U8 播放地址
- ⬇️ **视频下载** - 支持单个和批量下载视频
- 🔐 **加密支持** - 支持 AES-128 加密的 M3U8 文件
- 🔄 **并发下载** - 最多支持 3 个视频同时下载
- 📊 **进度显示** - 实时显示下载进度和速度
- 🔍 **状态筛选** - 可按状态筛选视频列表
- 💾 **配置保存** - 保存下载路径和认证信息

## 技术栈

| 层级 | 技术 |
|------|------|
| 前端框架 | Vue 3 + TypeScript |
| 构建工具 | Vite 6 |
| 桌面框架 | Tauri 2 |
| 浏览器引擎 | headless_chrome |
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
# Xcode Command Line Tools
xcode-select --install
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

# 或使用 npm
npm install
```

### 3. 下载 FFmpeg 资源（用于打包）

项目需要将 FFmpeg 打包到应用中：

```bash
cd src-tauri
node scripts/download-ffmpeg.cjs
```

## 开发运行

### 1. 启动开发服务器

```bash
# 在项目根目录运行
pnpm tauri dev

# 或使用 cargo 直接运行
cargo tauri dev
```

这将：
- 启动 Vite 开发服务器（端口 1420）
- 编译并运行 Tauri 应用
- 启用热重载（修改前端代码自动刷新）

### 2. 开发模式特点

- 前端代码修改后自动热更新
- Rust 代码修改后自动重新编译
- 打开开发者工具（在 Tauri 窗口右键 -> 检查）

## 生产构建

### 1. 构建应用

```bash
# 构建生产版本
pnpm tauri build

# 或使用 cargo
cargo tauri build
```

构建产物位于：
- **macOS**: `src-tauri/target/release/bundle/dmg/` 或 `.tar.gz`
- **Windows**: `src-tauri/target/release/bundle/msi/`
- **Linux**: `src-tauri/target/release/bundle/deb/`

### 2. 签名（macOS）

```bash
# 证书签名
codesign --sign "Developer ID Application: Your Name" --timestamp --entitlements src-tauri/entenew src-tauri/target/release/bundle/dmg/web-spider_*.dmg

# 公证（用于 Gatekeeper）
xcrun altool --notarize-app --primary-bundle-id com.yangzhenguo.web-spider --username "your@email.com" --password "app-specific-password" --file web-spider_*.dmg
```

## 清理命令

### 清理 Cargo 缓存

```bash
# 清理所有构建产物
cargo clean

# 清理特定 target
cargo clean -p web-spider

# 清理整个 target 目录
rm -rf src-tauri/target
```

### 清理 Node 依赖

```bash
# 删除 node_modules
rm -rf node_modules

# 删除 pnpm-lock.yaml 并重新安装
rm pnpm-lock.yaml
pnpm install
```

### 完整清理（推荐在出现问题时执行）

```bash
# 1. 清理 Cargo
cargo clean

# 2. 删除 lock 文件（首次运行会重新生成）
rm -f Cargo.lock src-tauri/Cargo.lock

# 3. 重新安装依赖
pnpm install

# 4. 重新构建
pnpm tauri build
```

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

### Q: 依赖安装缓慢或失败

A: 使用国内镜像源：
```bash
# Rust 镜像
export RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
export RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup

# npm/pnpm 镜像
pnpm config set registry https://registry.npmmirror.com
```

## 项目结构

```
web-spider/
├── src/                          # Vue 前端源码
│   ├── main.ts                   # Vue 入口
│   ├── App.vue                   # 根组件
│   ├── types.ts                  # TypeScript 类型定义
│   └── components/
│       ├── ScraperPage.vue       # 主爬取界面
│       ├── ConfigPage.vue        # 设置页面
│       └── LogPopup.vue          # 日志弹窗
├── src-tauri/                    # Tauri 后端源码
│   ├── src/
│   │   ├── lib.rs                # Tauri 入口
│   │   ├── main.rs               # 应用入口
│   │   ├── commands/             # 命令处理
│   │   │   └── mod.rs
│   │   ├── models/               # 数据模型
│   │   │   └── mod.rs
│   │   └── services/             # 业务逻辑
│   │       ├── mod.rs
│   │       ├── scraper.rs        # 爬虫服务
│   │       └── downloader.rs     # 下载服务
│   ├── tauri.conf.json           # Tauri 配置
│   ├── Cargo.toml                # Rust 依赖配置
│   └── scripts/
│       └── download-ffmpeg.cjs   # FFmpeg 下载脚本
├── package.json                  # Node 依赖配置
├── pnpm-lock.yaml
└── vite.config.ts                # Vite 配置
```

## License

MIT License
