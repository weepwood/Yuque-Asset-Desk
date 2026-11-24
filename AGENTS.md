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
