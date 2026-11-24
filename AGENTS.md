# Yuque Asset Desk — AI 开发协作说明

## 项目定位
- Rust + GPUI Kit 0.9.0 + SQLite 原生桌面上传工具；不引入 Electron 或 WebView。
- 保留现有数据库目录、assets 表及 provider + owner_id + sha256 去重约束。
- Demo 模式仅生成本地 file:// 链接；语雀模式使用非公开网页接口，不保证外链长期有效。
- 不在源码、日志、测试夹具、提交或聊天中保存真实 Cookie / CSRF Token。

## 开发与验证
- 优先检查 git status，保留用户未提交改动；中文 Git commit。
- 日常：python3 scripts/static_check.py、git diff --check、cargo check、cargo test。
- 高风险（上传、账号、数据库）：增加隔离、去重、错误恢复测试，使用临时数据库和 Demo 模式。
- 发布：三平台 CI（Windows x64、macOS arm64/x64）、真实窗口验收、版本和产物校验；未通过不得宣称已发布。
- 优先使用成熟 GPUI Kit 组件；新依赖需检查许可证、维护状态和跨平台构建。
- 当前交接及未验证事项见 docs/development-status.md。

## V0.4 开发约束
- Cargo.lock 必须提交；使用 cargo check --locked --all-targets、cargo test --locked 与 cargo build --locked。
- 真机冒烟测试使用隔离目录 YUQUE_DATA_DIR 与 YUQUE_MODE=demo，禁止自动使用真实凭据测试。
- 账号密钥只交给系统凭据库；未知上传结果不自动重试，远端预览不得附加账户凭据。
- SQLite schema 只做向后兼容的增量变化，任务恢复必须保持账号范围；暂停/取消仅文件边界，避免虚报实时中断。

## 第二轮开发约束
- UI 与控制器入口位于 src/desk/，main.rs 仅负责启动；跨模块变更先检查账号范围与状态所有权。
- 任务中心的 running（非当前活跃任务）表示未知远端结果；禁止自动重试。HTTP 请求体字节数不等于服务端确认。
- 会话检查必须由用户触发且验证账号 ID；不记录 Cookie、Token 或响应体；未经授权不发送实际上传。
- 仅在独立数据目录、Demo 模式使用 YUQUE_KEYCHAIN_SMOKE=1 测试系统凭据库，不读写真实账号凭据。
- 预览缓存清理仅作用于派生图片；不得删除 clipboard 源文件、SQLite 资源或任务。

## 离线相册与任务恢复约束
- 相册默认为响应式平铺；右侧详情按需展开。复制链接只读取 SQLite 原始 URL，不读取图片、不请求语雀。
- 上传成功/复用记录后，由真实本地来源生成最长边 160px、JPEG 质量 45 的永久微缩图，目录为数据根目录 album-previews；provider/owner_id/sha256 隔离，不按 URL 变化失效，不做 LRU 淘汰。
- 旧历史可从旧版预览缓存、Demo 原文件或当前资源账号下已完成任务的同名本地源恢复；原文件必须校验 SHA-256。无来源时保留占位和复制链接，禁止自动下载远端图片。
- 不主动删除既有 clipboard 文件、旧预览或用户原文件；相册微缩图属于持久化数据而非临时缓存。明确用户操作才可打开外链浏览器。
- 上传请求禁用自动重试；结果未知任务排除批量恢复，需人工核实；限速 YUQUE_UPLOAD_KIB_PER_SEC，取消仅为协作式中止。
- QA：cargo test --locked --features qa；macOS 本地 cargo build --locked --features qa 后 python3 scripts/visual_qa.py。QA 数据必须隔离，无真实账号或网络上传。
