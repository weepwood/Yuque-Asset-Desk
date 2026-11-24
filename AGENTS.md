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

## 相册深化与备份约束
- 相册使用 GPUI ListState 按行虚拟化，每页 150 项 SQLite 游标查询；等宽和等高拼图布局算法位于 src/desk/album.rs。尺寸 96–240px，日期按数据库 UTC 字段分组。
- 收藏与感知哈希均为增量表，按资源 ID 关联；相似检索只在所选资源同账号内匹配本地 dHash，不下载远端原图；历史微缩图按需索引。
- 相册备份是全部账号的 SQLite 一致性快照和派生预览，不含系统密钥库或原始上传/剪贴板文件；必须导出到数据目录以外的新文件夹，禁止覆盖当前数据库。恢复使用新的 YUQUE_DATA_DIR，先确认数据再切换；不自动重传任务。
- 界面变化使用 scripts/visual_qa.py 验证；QA 原生滚轮和点击测试优先使用 debug_selector 获取实际布局边界，禁止为了测试放宽业务约束。

## 沉浸式相册与原生桌面集成
- 瀑布流使用最短列布局，按日期独立分区；原生 ScrollHandle 驱动滚动，按实际视口加 160px 预渲染范围虚拟化，尺寸/比例变化以资源 ID 锚点恢复。不要自研滚动物理。
- 保持 150 项游标分页和永久本地微缩图；每 12 项批量更新预览状态；已索引图片读取尺寸头，不重复计算 dHash。性能验收用隔离合成 JPEG，不访问真实图片或账号。
- macOS/Windows 使用 tray-icon 和 global-hotkey，托盘上传必须主动点击；全局快捷键仅唤起相册。关闭窗口仅在托盘可用时最小化。默认运行可通过 YUQUE_DESKTOP_INTEGRATION=0 禁用；隔离目录默认不注册系统集成，显式 YUQUE_DESKTOP_SMOKE=1 才允许测试。
- QA 性能脚本 scripts/album_perf.py 依赖 Python Pillow，指标为 CPU dispatch/draw 耗时和进程采样 RSS/CPU，不等于真实显示 FPS 或应用 GPU 利用率。不要宣称未实测的硬件性能。

## 连续相册、内存缓存和偏好持久化
- 资源查询按 150 项 keyset 批次追加，滚动接近已加载末尾自动加载；同一次查询保留选择，筛选或账号变化重置游标，防止旧异步结果污染。批量选择上限 150，保持既有 SQLite 标签事务约束。
- GPUI ListState 原生滚动、未变化行保留测量；Masonry 复用布局和资源 ID 锚点，按 y 起始位置二分定位。不得用滚动事件中的旧可视区直接触发分页；延迟读取实际逻辑滚动锚点。
- 已有 JPEG 预览先读取本地尺寸头；缺失预览优先可视区及附近每批 12 项，禁止网络补图。AlbumImageCache 限制 192 个解码条目，淘汰调用 GPUI drop_image，永久微缩图不删除。资源元数据仍随连续加载累积。
- desk_preferences 增量表保存主题、布局、密度、排序、收藏筛选、日期分组、正常窗口尺寸与快捷键 U/I/off。写入防抖，退出补存；QA 截图和性能参数不覆盖用户配置。
- 系统快捷键冲突不得抢占其他进程；切换 U/I/off 后重启生效。剪贴板只在用户主动操作时读取，隔离测试使用 TestAppContext 私有剪贴板。未经授权不注入物理按键、不修改系统辅助功能权限。
- 性能：cargo build --release --locked --features qa; python3 scripts/album_perf.py。原生帧回调有 6 秒超时，回调间隔不等于硬件呈现 FPS；ioreg 为整机 GPU 利用率。真实系统操作与模拟交互必须区分。

## v0.4.0 发布候选
- 无选择时隐藏批量操作行；常用相册操作复用 GPUI 图标和 tooltip。沉浸图片墙隐藏侧栏/详情栏，保留显式退出及独立系统全屏按钮；选择资源详情退出沉浸模式。
- 版本以 Cargo.toml 为准，Cargo.lock 同步；scripts/package_release.py 统一生成三架构版本化 ZIP、macOS Info.plist、ad-hoc 签名与 SHA256。macOS 非公证，Windows 为便携 ZIP，不宣称有安装向导。
- 发布门槛：同一候选 SHA 的 main 三平台 CI 成功，下载并核验所有归档及 SHA256，再创建 v0.4.0 Tag/Release；未完成真机项目如实记载，不自动触发发布。禁止使用真实账号或系统剪贴板自动上传测试。

## v0.4.0 跨平台发布复验
- Windows 文件 URL 的路径往返可能去除扩展长度路径前缀；比较路径时两侧均 canonicalize，并实际读取文件及核对 SHA-256，禁止通过跳过测试掩盖平台差异。
- 下载发布产物时只接受最终候选 CI 的三个明确目标 artifact；先比对 CI 原始 SHA256 文件，再检查 ZIP、二进制架构及应用签名，避免 verify-only 重新生成校验文件掩盖损坏。

## v0.4.0 正式发布（2026-11-25）
- 根据用户最终确认，V0.4.0 仅发布 Windows x64 与 macOS ARM64 两个 ZIP 及各自 SHA256 文件，Intel Mac 不纳入本次发布门槛。未修改既有 CI 矩阵。
- 正式 Tag v0.4.0 固定于已验收源码 6f485d8，CI 45432243907 两个目标 job 均 success。下载 CI 原始产物独立验证 SHA256、ZIP、架构、macOS 签名，再以 GitHub 返回的资产 digest 核实全部四个资产。不得移动 Tag 或混用其他提交产物。
- macOS ARM64 CI 应用实际隔离启动通过，Windows 仅 CI 与归档/PE 架构验证；不宣称 Windows 真机启动、Apple 公证、系统通知送达或真实语雀上传已验收。
