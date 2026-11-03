# Yuque Asset Desk

Rust + **GPUI Kit 0.9.0** + SQLite 的跨平台原生图片与附件上传工作台（v0.3.0）。

本次迁移将桌面 UI 从 Slint 改为 GPUI Kit。上传器、SHA-256 去重、SQLite 记录和 Markdown/URL 输出仍沿用之前的 Rust 业务模块。

## 支持平台

- Windows 10/11 x64：MSVC 工具链、Visual Studio 2022 C++ 开发工具和 CMake
- macOS 15+：Apple Silicon / Intel，Xcode Command Line Tools
- Rust 1.92 或更高；第一次编译需要下载 GPUI Kit 和原生渲染依赖

本版本不依赖 Electron、Node.js 或 WebView。但 GPUI Kit 的最终内存、启动耗时与发行包大小尚需实测，不能直接使用 Slint 版的性能数据。

## 构建与运行

```bash
cargo run --release
cargo test
```

GitHub Actions 将尝试在 Windows x64、macOS ARM64、macOS Intel 环境构建；生成的未签名 ZIP 属于测试构建，不能视为已经通过真机验收。macOS 正式分发需要 Apple 签名与公证。

## 版本功能

| 功能 | 迁移状态 |
| --- | --- |
| GPUI Kit 原生界面、搜索输入框、主题按钮 | 已编写 |
| 多文件选择、串行上传任务队列 | 保留 |
| 语雀 Web Session 上传适配器 | 保留，待真实账号实测 |
| 本地演示模式（仅 file:// 链接） | 保留 |
| SQLite 历史与 SHA-256 文件去重 | 保留 |
| 搜索、图片筛选、复制 URL/Markdown | 保留 |
| Slint UI 和构建脚本 | 移除 |
| 系统托盘 | 暂未移植 |
| 原生文件拖放、截图自动粘贴 | 尚未实现 |

当前不将工具描述为已经支持系统托盘：GPUI Kit 并不直接复用 Slint 的 SystemTrayIcon。后续需要单独实现跨平台托盘适配器。

## 语雀会话配置

默认是本地演示模式，不进行网络上传，只返回本地文件 `file://` URL。

若要启用语雀适配，请在本机运行环境设置：

```text
YUQUE_MODE=yuque
YUQUE_USER_ID=你的数字用户 ID
YUQUE_CTOKEN=自己的 CSRF Token
YUQUE_COOKIE=自己的登录会话 Cookie
YUQUE_MAX_FILE_MB=25
```

这些会话凭据不要提交 GitHub，不要发在聊天里。程序不会把这些值写入 SQLite。未来应改为系统 Keychain / Credential Manager。
语雀 Web 上传接口并非承诺稳定的正式 OpenAPI，需验证鉴权、文件限制和外链可访问性。

## 数据与低资源策略

本版本沿用 `directories::ProjectDirs::from("com", "Weepwood", "YuqueAssetDesk")` 的数据路径及 `assets.db` 数据库结构。迁移前建议先退出 Slint 旧版并备份数据库。

Rust 后台上传任务使用 GPUI 的后台执行器处理阻塞 I/O，避免渲染线程被 HTTP 请求卡住。上传队列长度有限，历史仅加载最近 300 条，不默认解码缩略图，也没有持续轮询和无限动画。

### 目录

```text
src/
  main.rs       GPUI Kit Dashboard Entity + Render + 后台队列
  config.rs     凭据和存储模式
  storage.rs    SQLite
  uploader.rs   流式上传与 SHA-256
  format.rs     URL 与 Markdown
docs/
  GPUI_MIGRATION.md
```

- [GPUI Kit](https://gpui-kit.com/)
- [平台安装要求](https://gpui-kit.com/docs/installation/)
- [任务与异步编程](https://gpui-kit.com/docs/task/)
