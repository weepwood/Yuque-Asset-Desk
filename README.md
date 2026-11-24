# Yuque Asset Desk

基于 **Rust + GPUI Kit 0.9 + SQLite** 的轻量跨平台语雀图片与文件上传桌面客户端。

> **当前正式版本：[v0.5.1](https://github.com/weepwood/Yuque-Asset-Desk/releases/tag/v0.5.1)。**

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

图形界面：设置 → 语雀账号 → 管理（v0.5.1）。
- 可选填写 [个人访问 Token](https://www.yuque.com/settings/tokens) 并点击「识别账号」，通过官方 OpenAPI 获取 ID、昵称、登录名；不持久化个人 Token。
- 填写自己的网页 Cookie；其中包含 yuque_ctoken 时无需另填 CSRF Token。点击「验证并保存账号」，重新验证网页会话并核对可选的个人 Token 所属 ID，避免错绑。没有个人 Token 时直接通过 Cookie 识别；网络或身份校验失败不保存。
- 个人 API Token 不是网页上传凭据，不能单独上传。旧账号凭据格式、历史和数据库保持兼容。团队 Token 不用于个人上传。

macOS bash（原有开发环境变量方式，仍需显式 ID）：

```bash
export YUQUE_MODE=yuque
export YUQUE_USER_ID=123456
export YUQUE_CTOKEN='replace-me'
export YUQUE_COOKIE='_yuque_session=...; yuque_ctoken=...'
cargo run --release
```

Windows PowerShell 使用 `$env:KEY="value"` 设置环境变量。不要把个人 API Token、Cookie 和 CSRF Token 提交到 GitHub、Issues 或聊天中。

## 迁移范围

已实现源码：GPUI Kit 窗口、上传工作台、系统文件选择、多文件串行任务、SQLite 资源历史、文件去重、复制 URL / Markdown、搜索与筛选、状态提示。

**已支持：** 原生托盘、文件拖放、主动粘贴、全局快捷键、图形化账号管理和系统凭据库。

**尚未完成真机验收：** Windows 实际运行、完整辅助功能、真实语雀账号上传、CDN 长期有效性及 Apple 公证。

保留已有 SQLite 目录、字段和去重约束，不删除旧上传历史。GPUI Kit 的 GPU 资源占用与 Slint 不同，不能假设更换后必然更省内存。

## CI

GitHub Actions 仅覆盖 Windows x64、macOS Apple Silicon。V0.5.0 候选使用 Windows 便携 ZIP（未签名）与 macOS ARM64 DMG（应用 ad-hoc 签名、未经公证）；构建成功不等于完成终端设备验收。参见 [构建工作流](.github/workflows/build.yml)。

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
默认平铺图片卡片，直接复制原始链接，也可切换列表。成功上传后保存最长边 160px 的低质量 JPEG 微缩图于数据目录 `album-previews/`，不下载远端图片，不按数量淘汰；旧记录尽可能由现有本地数据恢复。缺失微缩图不影响复制链接。正式 v0.5.1 放大预览仍为低清微缩图。v0.6.0 开发候选新增按需原图预览（见下方），默认仍使用离线微缩图。既有数据库与用户源文件保留。

### 相册浏览与备份
- 等宽网格、等高拼图和列表；96–240px 密度调节、UTC 上传日期分组、收藏、悬停 Markdown 操作。
- 相似图片：基于本地 160px 微缩图的 dHash，限同一账号，最多 150 条结果。首次检索可后台补齐已有本地微缩图的索引，无法替代语义搜索。
- 点击左栏「备份相册」，选择应用数据目录以外的位置，生成 `YuqueAlbum-...` 文件夹。备份包含全部账号的链接、标签、收藏、任务及派生预览，不包含系统凭据库或原始上传文件。备份本身包含私人链接及本地任务路径，请妥善保护。
- 迁移：复制备份至新目录，退出当前应用；通过 `YUQUE_DATA_DIR` 指定该新目录启动程序，例如 macOS：`YUQUE_DATA_DIR="/path/to/YuqueAlbum" YUQUE_MODE=demo ./target/debug/yuque-asset-desk`。在新机器上重新配置语雀会话；不要覆盖已有数据目录，不自动恢复上传任务。Demo 文件链接仍指向原路径，微缩图可离线查看。

### 沉浸式相册和桌面操作
紧凑顶部工具栏、等宽 / 等高 / 瀑布流 / 列表模式，瀑布流采用最短列布局、保留图片比例和滚动锚点。日常浏览仍完全读取本地微缩图。

macOS / Windows 原生托盘：打开相册、主动上传剪贴板、退出。全局快捷键 macOS Cmd+Shift+U、Windows Ctrl+Shift+U 仅打开工作台，不读取剪贴板。托盘可用时关闭窗口会最小化；实际注册可能因系统快捷键冲突失败。可设置 YUQUE_DESKTOP_INTEGRATION=0 禁用。完成通知取决于系统权限；不包含图片内容或链接。

本地性能复现（macOS，Python 需要 Pillow）：
```bash
cargo build --locked --features qa
python3 scripts/album_perf.py
```
报告写入 dist/qa/album-perf.json；测试使用 10,000 个合成 JPEG 和 150 项分页。CPU 渲染耗时不等于屏幕 FPS。系统托盘和快捷键的注册冒烟不读取剪贴板、不使用真实凭据。

### 连续浏览与偏好
相册接近已加载末尾时自动追加下一批 150 项，也保留「加载更多」按钮。过滤或账号变化后重新查询，批量选择最多 150 项。已有微缩图预读取比例，缺失预览优先处理可见资源。GPUI 解码缓存采用 192 项 LRU，不清除磁盘永久微缩图。

主题、布局、卡片尺寸、排序、收藏过滤、日期分组、正常窗口大小和快捷键方案保存于 SQLite。侧栏快捷键设置可以循环 U / I / 禁用，重启生效。默认 macOS Cmd+Shift+U / Windows Ctrl+Shift+U；备用为对应 I 键。

Release QA 性能和桌面测试（macOS；Pillow）：
```bash
cargo build --release --locked --features qa
python3 scripts/album_perf.py
python3 scripts/desktop_qa.py
YUQUE_VISUAL_PROFILE=release python3 scripts/visual_qa.py
```
报告区分 CPU 绘制耗时、原生帧回调、整机 GPU 样本。原生回调未到达会超时记录，不能将 CPU 耗时或回调频率当作屏幕真实呈现 FPS。

### v0.4.0 下载
正式发布仅提供 Windows x64 便携 ZIP 与 macOS ARM64 应用 ZIP，均附 SHA256。macOS 为 ad-hoc 签名、未公证；Windows 未签名。详见 [Release](https://github.com/weepwood/Yuque-Asset-Desk/releases/tag/v0.4.0)。

### V0.5.0 UI 开发
V0.4.0 已发布版本保持不变。开发分支的 V0.5.0 聚焦内容优先相册、语义主题、原生菜单和独立设置；设计文档见 [docs/ui-design.md](docs/ui-design.md)。视觉脚本使用 Python Pillow 生成隔离合成测试图片（先安装 Pillow），不使用个人数据。

### 检查更新
在设置 → 关于与更新中手动检查 GitHub 正式版本。发现新版本后，下载适配平台的 DMG 或 ZIP 与 SHA256 文件，核对摘要，退出应用并安装或替换程序文件。不会自动安装或修改本地数据。

### V0.5.0 安装包与品牌资源

<img src="assets/branding/app.png" alt="Yuque Asset Desk 应用图标" width="128">

macOS ARM64：打开 DMG，将左侧应用拖到右侧 Applications，弹出磁盘映像，再从应用程序启动。安装包的应用图标、版本、签名和 Finder 布局由构建脚本核验。ad-hoc 签名不等于 Apple 公证。Windows x64：解压便携 ZIP，运行 EXE。两个产物均附 SHA256。旧版 v0.4.0 ZIP 保持不变。

开发者可运行 `python3 scripts/generate_branding.py`（Pillow 11+，macOS 字体），重新生成自有图标和安装背景；macOS 打包使用 `python -m pip install -r scripts/requirements-dmg.txt`。

### 存储与卸载
设置 → 存储与数据 → 管理，可统计空间、打开数据目录、清理无引用暂存文件、删除已保存凭据或在备份后选择完全重置。普通卸载保留本地数据；永久微缩图不会被轻量清理删除。详细步骤、残留及安全限制见 [卸载与数据管理](docs/uninstall.md)。

### v0.6.0 原图预览（开发候选）
双击进入预览后点击「本地原图」读取仍存在、SHA256 与记录一致的源文件；语雀资源可主动点击「在线原图」。仅支持常见 `https://cdn.nlark.com/yuque/` CDN 链接，不携带账号凭据、不跟随重定向；若服务端转换文件导致哈希不同、CDN 拒绝或地址不支持，可在浏览器打开原链接。网络使用现有代理/系统路由，不主动请求远端缩略图。
支持静态 PNG/JPEG/WebP/GIF 首帧/BMP；输入至多 32 MiB、1600 万像素、单边 8192，显示最长边至多 2560px（超出时标示原始和预览尺寸）。高清稳态只保留当前一张内存图，切换图片/关闭即释放，替代来源加载失败保留当前图（成功替换时短暂共存两张），不修改源文件、不新增持久缓存、不覆盖永久微缩图。左右切图可按当前筛选与账号范围追加分页，单次最多检查九页；底部最多七张邻近离线缩略图。无实际语雀账号/CDN 真机验证，Windows 需 CI。

预览中可点击「离线预览」释放高清；上传完成的相册刷新延迟到返回时执行。
