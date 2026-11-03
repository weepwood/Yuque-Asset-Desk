# Yuque Asset Desk — Rust + Slint + SQLite（0.2.1 源码原型）

这是从此前的 Node.js 本地网页原型迁移到 **Rust + Slint 1.18 + SQLite** 的第一阶段源码。
面向 **Windows x64 / macOS Apple Silicon / macOS Intel**。没有 Electron、Chromium、Node.js 常驻服务或 WebView。

> **交付状态：** 此压缩包是源码项目，不是已签名的 macOS/Windows 安装包。由于当前构建环境没有 Rust 工具链，且无法访问 crates.io，**本轮未能执行 cargo check、cargo test 或产出编译后的可执行程序**。不要把 CI 配置视为已经通过的构建证明。

## 已编写功能

- 使用 Slint 编写的原生 UI；系统托盘隐藏和重新打开。
- 使用系统文件选择器支持多文件选择，队列串行上传；避免并发引起不必要的内存峰值。
- Rust 后台工作线程执行 SHA-256 计算与流式 multipart 上传；UI 线程不执行上传任务。
- SQLite WAL 持久化上传记录，以 `provider + owner_id + sha256` 唯一约束去重。
- 资源历史与名称搜索；复制 URL 或 Markdown 图片/附件链接。
- 本地演示模式和语雀网页端上传适配器；语雀响应必须包含 HTTPS URL 才认定上传成功。
- 基于 GitHub Actions 的 Win x64 / Mac ARM64 / Mac x64 构建配置（尚未运行）。

## 快速运行（需要 Rust stable）

```bash
cargo run --release
```

首次编译需要联网下载 Cargo 依赖；Slint 编译时间和缓存体积可能较大，但发布后运行无需 Rust 工具链。

### Windows

```powershell
winget install Rustlang.Rustup
cargo run --release
```

### macOS

安装 Xcode Command Line Tools 和 Rust stable 后：

```bash
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo run --release
```

## 语雀上传模式

语雀**文件上传路径来自第三方研究的非公开网页端接口**，不是官方承诺长期稳定的开放 API，实际请求格式可能发生变化。

**绝不要把 Cookie、Session、CSRF Token 保存到 Git 仓库或发送到聊天。**
当前原型采用运行时环境变量注入，不会将 Cookie/Token 写到 SQLite、日志或配置文件。

Windows PowerShell（仅示例；实际值从自己的已登录浏览器会话合法取得）：

```powershell
$env:YUQUE_MODE="yuque"
$env:YUQUE_USER_ID="123456"         # 改为自己的数字 ID
$env:YUQUE_CTOKEN="replace-me"        # 改为自己的 CSRF Token
$env:YUQUE_COOKIE="_yuque_session=...; yuque_ctoken=..."  # 改为自己的 Cookie
cargo run --release
```

macOS Terminal：

```bash
export YUQUE_MODE=yuque
export YUQUE_USER_ID=123456
export YUQUE_CTOKEN='replace-me'
export YUQUE_COOKIE='_yuque_session=...; yuque_ctoken=...'
cargo run --release
```

可选：`YUQUE_MAX_FILE_MB=25`（默认 25，最大 1024；**服务端仍可能有更小的限制**）。

实际请求（待账号实测）为：

```http
POST https://www.yuque.com/api/upload/attach?attachable_type=User&attachable_id=...&type=image|attachment|video&ctoken=...
Cookie: ...
x-csrf-token: ...
Origin: https://www.yuque.com
Content-Type: multipart/form-data
```

⚠️ **Cookie 过期、接口调整、鉴权失败、CDN 防盗链或外部引用限制**均可能导致上传或外链失效。请先用无敏感信息的小文件测试。上传成功不等于链接在所有博客可公开访问或会长期有效。

## 本地演示模式（默认）

```bash
cargo run
```

演示模式**不会发起任何上传请求**，只把本地原文件的 `file://` URL 记入数据库。因此不能将演示链接作为互联网图床地址使用；移动/删除原文件后链接也会失效。

## 存储位置

使用操作系统的应用数据目录，由 `directories::ProjectDirs` 决定，通常类似：

- macOS：`~/Library/Application Support/com.Weepwood.YuqueAssetDesk/assets.db`（具体路径以库生成结果为准）
- Windows：`%LOCALAPPDATA%\\Weepwood\\YuqueAssetDesk\\data\\assets.db`（具体路径以库生成结果为准）

数据使用 SQLite，WAL 运行时可能另有 `-wal`、`-shm` 文件。若要完整备份，请先退出应用。
不缓存大图缩略图，也不自动复制上传文件到本地数据库目录。

## 原生拖拽和图片粘贴

Slint 1.17 增强了内部拖拽，但不能据此推断 **Finder/Explorer 原生文件列表拖拽** 已可直接使用。此源码原型先采用稳定的 `rfd` 系统文件选择器。
**从系统文件管理器直接拖入、截屏后剪贴板图片粘贴、全局热键、语雀登录窗口与 Keychain 凭据存储**未实现。

后续迭代应优先添加 macOS/Windows 原生文件投递适配器、OS Keychain / Credential Manager、安全的账号配置 UI，并验证外链的公开可访问性。建议不要把网页 Cookie 写进 SQLite。

## 低资源策略

- Slint 使用 Winit + FemtoVG，不包含 WebView、Skia、Electron。
- 队列默认串行、工作线程在无任务时阻塞等待，不做 16ms 轮询。
- 文件读取使用 64 KiB SHA-256 缓冲；HTTP Multipart 从文件流式上传。
- UI 只加载最近 300 条资源元信息，不做图片预解码与缩略图缓存。
- Release 配置 LTO、尺寸优化与 strip；具体安装包大小和内存峰值需要在两台真实系统上测量。

## 测试与构建

```bash
cargo fmt --check
cargo check
cargo test
cargo build --release
```

`.github/workflows/build.yml` 定义三平台构建产物，仅供上传到 GitHub 后执行。由于当前环境无法生成 `Cargo.lock`，首次在联网环境运行 `cargo generate-lockfile` 并将生成的文件提交版本库，然后将 CI 改为 `--locked` 可提高复现性。macOS `.app` 为未签名版本，首次打开可能触发 Gatekeeper；正式分发需要 Apple 开发者签名和公证。Windows 也建议正式签名。

## Slint 授权说明

本项目自己编写的源码采用 MIT 协议，但 Slint 框架本身采用多重许可。按其 **Royalty-Free Desktop** 条款分发时，通常需要使用 `AboutSlint` 等方式署名。本项目已经在“关于应用 / Slint”窗口加入官方 `AboutSlint` 组件。正式发行前请核对 [Slint 官方协议](https://slint.dev/community)。

## 后续规划

1. 编译期验证：cargo check + Slint UI 构建 + GitHub CI。
2. 授权体验：用 OS Keychain 保存账号凭据、账号测试和刷新提示。
3. 原生文件拖拽、剪贴板图片上传、菜单栏上传快捷入口。
4. 引用关系索引：追踪 Obsidian / Typora 的本地 Markdown 文章与资源，支持失效检测和图床迁移。
5. 可插拔存储后端（R2/OSS/S3），语雀作为可选适配器而不是唯一后端。

## v0.2.1 原生界面与动效

本次界面调整以原生 GUI 的事件驱动为原则，不使用持续刷新的定时器。
- 重新布局深色导航栏、资源统计卡片、上传区域和历史列表；
- 上传按钮、导航与文件列表增加约 130–170ms 的 hover/press 背景过渡；
- 首次打开窗口上传卡片以 320ms 单次淡入；
- SQLite 资源计数、队列长度、上传中状态与界面绑定；
- 可从侧栏或列表筛选图片，保留原有资源搜索及 Markdown 复制；
- 修复旧版 CI 的 `AppTray::on_clicked` 编译错误：只使用托盘菜单的“打开”操作，兼容当前锁定的 Slint 版本。

详细设计与验收建议参见 [docs/UI_MOTION.md](docs/UI_MOTION.md)。UI 改动不代表语雀接口已完成真实账号联调。
