# Yuque Asset Desk

基于 **Rust + GPUI Kit 0.9 + SQLite** 的轻量跨平台语雀图片与文件上传桌面客户端。

> **迁移状态：v0.3.1 GPUI Kit 初版。** 已将原有 Slint UI 入口更换为 GPUI Kit View + Button + Input，上传后端和数据库保留。需经过 CI 与真机验证，才可正式发布。

## 技术架构

- **GUI**: [GPUI Kit](https://gpui-kit.com/docs/getting-started/) 0.9.0，基于 GPUI 的原生 GPU 渲染。
- **核心**: Rust，SHA-256 去重，reqwest 流式 multipart 上传。
- **存储**: SQLite + rusqlite，保留原数据路径和数据表，按存储提供方 + 用户 + SHA-256 去重。
- **任务**: GPUI Task / background_spawn；逐个上传并呈现结果，不阻塞 UI 的文件哈希及网络上传。
- **输出**: URL 与 Markdown 图片或附件链接复制到系统剪贴板。
- **界面**: 导航、上传入口、任务计数、历史资源列表、名称搜索与图片筛选。

## 环境

- Windows 10+、Rust 1.92+（MSVC）、Visual Studio C++ Build Tools + Windows SDK + CMake。
- macOS 15+、Xcode Command Line Tools、Rust 1.92+。
- 不使用 Electron、Chromium、Node.js 或 WebView。

[GPUI Kit 安装指南](https://gpui-kit.com/docs/installation/)

## 开发

```bash
rustc --version
cargo check
cargo test
cargo run --release
```

首次构建 GPUI 依赖较多；不能从安装包大小推断构建缓存、运行内存或冷启动耗时。

## 模式与语雀凭据

默认 Demo 模式，不上传至网络，只记录本地 `file://` 链接；此类链接不能用于公开博客。

语雀上传功能使用第三方逆向研究的非公开 Web API，不是语雀正式承诺的稳定接口。请仅使用自己的合法授权会话，并保留原文件。

macOS bash（示例）：

```bash
export YUQUE_MODE=yuque
export YUQUE_USER_ID=123456
export YUQUE_CTOKEN='replace-me'
export YUQUE_COOKIE='_yuque_session=...; yuque_ctoken=...'
cargo run --release
```

Windows PowerShell 使用 `$env:KEY="value"` 设置环境变量。不要把 Cookie 和 CSRF Token 提交到 GitHub、Issues 或聊天中。

## 迁移范围

已实现源码：GPUI Kit 窗口、上传工作台、系统文件选择、多文件串行任务、SQLite 资源历史、文件去重、复制 URL / Markdown、搜索与筛选、状态提示。

**暂未迁移和验证：**
- Slint 原有 SystemTrayIcon 不能直接复用；当前 GPUI 版本没有托盘入口。
- Finder / Explorer 文件拖放、粘贴截图、全局快捷键。
- 窗口动效、无障碍、DPI 和运行内存的跨平台真机测试。
- 图形化账号管理及 Keychain / Credential Manager 凭据保管。
- 真实语雀账号端到端测试、CDN 公开外链/防盗链验证、安装包签名与公证。

保留已有 SQLite 目录、字段和去重约束，不删除旧上传历史。GPUI Kit 的 GPU 资源占用与 Slint 不同，不能假设更换后必然更省内存。

## CI

GitHub Actions 覆盖 Windows x64、macOS Apple Silicon 和 macOS Intel。构建产物是未签名 ZIP，不等同于完成终端设备验收。参见 [构建工作流](.github/workflows/build.yml)。

更多文档：https://gpui-kit.com

## v0.3.1 稳定性与性能修复

- 修复 GPUI Kit 0.9.0 列表滚动 API：显式引入 `ScrollableElement`，改用 `overflow_y_scrollbar()`。
- 搜索/图片过滤改为先在 SQLite 过滤再应用 150 条上限，避免遗漏较旧的匹配资源。
- 渲染时不再每次调用 SQLite `COUNT(*)`，上传完成及过滤操作时更新缓存总数。
- 上传反馈显示当前完成项数 / 队列项数；语雀凭据缺失时在文件读取前报错。
- 将运行窗口视为原生 GPUI 客户端；此阶段**仍无托盘支持**。

### 更多性能与交互改进

- 资源行悬停态与层级化文字、上传区标题样式、当前导航选中态。
- 上传期间禁用重复触发文件选择，避免任务队列覆盖。
- 上传状态通过约 160ms 的 GPUI Base 状态过渡呈现；遵循系统减少动态效果偏好，不引入无限动画。
- WAL 模式仅在数据库初始化时设置；SQL 搜索不遗漏较早资源，新增回归测试。
- 后台上传线程遇到无语雀凭据会在读取文件前拒绝，并有单元测试覆盖。

## 本地开发中的资源管理改进

- 默认展示当前账号及模式的资源，显式切换到全部账号可查看旧记录。
- 资源列表使用每页 50 条的游标分页，后续查询在后台执行。
- 上传批次显示新增、复用和失败数量，并可手动重试失败项；网络结果不确定时应先检查语雀网页端，避免重复上传。
- 开发协作规范见 [AGENTS.md](AGENTS.md)，当前验收范围见 [开发交接](docs/development-status.md)。

### V0.4 开发阶段（未发布）

- 三栏资源工作台、浅色/深色切换、资源详情、网格/列表、标签、排序及本页批量 Markdown。
- 账号凭据使用 GPUI 系统密钥库接口；凭据已保存不代表会话有效，真实接口需独立验证。
- 文件拖放及剪贴板导入，SQLite 保存上传任务；暂停/取消在文件之间生效，恢复需手动触发。
- 本地缩略图缓存，远端图片需主动加载，不发送账户凭据。
- 可用 `YUQUE_DATA_DIR` 指定独立测试目录；`YUQUE_UPLOAD_INTERVAL_MS` 调整语雀文件之间的请求间隔（默认 350ms）。
- 当前开发验收与限制见 [开发交接](docs/development-status.md)。

### 第二轮工作台优化（开发中）
- 独立上传任务中心、历史失败详情、单任务恢复及 HTTP 文件读取进度。
- 账号面板提供主动只读会话检查，不保证上传接口权限。
- 可折叠详情栏、窄窗口布局、图片/视频/附件分类、1600px 大图预览、批量复制与派生缓存清理。
- 代码入口已拆分至 `src/desk/`，最新测试和限制见 [开发交接](docs/development-status.md)。

### 离线资源相册
默认平铺图片卡片，直接复制原始链接，也可切换列表。成功上传后保存最长边 160px 的低质量 JPEG 微缩图于数据目录 `album-previews/`，不下载远端图片，不按数量淘汰；旧记录尽可能由现有本地数据恢复。缺失微缩图不影响复制链接。放大预览仍为低清微缩图；需要原图请主动打开浏览器链接。既有数据库与用户源文件保留。
