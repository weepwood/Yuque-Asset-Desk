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

## V0.5.0 UI 重构
- 视觉规范与组件审计见 docs/ui-design.md。src/desk/design.rs 为工作台语义色、尺寸和动效入口；Theme::update 同步 GPUI Kit 主题。src/desk/workspace.rs 集中导航、筛选/布局菜单、设置、卡片右键行为。禁止重复实现成熟基础交互。
- GPUI Kit 保持 0.9.0；使用 SidebarMenuItem、DropdownMenu、PopupMenuItem、ContextMenuExt、Progress，保留 Button/Input 等。默认全部资源及既有筛选/数据逻辑不变；图片卡片仅悬停/选中显示操作，右键菜单保留链接、Markdown、收藏、选择、详情、相似检索。
- 不修改既有数据库结构、离线微缩图、上传或凭据逻辑；V0.5.0 UI 开发阶段不自动发布。QA 使用 Pillow 生成原创合成素材，隔离数据目录、禁止真实账号和剪贴板读写；核对真实双主题、窄窗口截图及原生交互。

## V0.5.0 高级视觉精修
- 继续使用 GPUI Kit 0.9.0，原生 Slider/Switch/Empty/Notification；完整 Settings 组件经评估不套入第二层导航。复制通知仅应用内，不新增系统通知权限或网络请求。
- design.rs 统一间距、字体、主按钮/浮层色和动效，album.rs 共用布局间距；保持离线预览、虚拟化、缓存及现有数据库。详情栏窄窗口必须保持关闭入口可见，100% 放大预览必须完整适应窗口并保留比例。
- scripts/visual_qa.py 默认完整截图，YUQUE_VISUAL_ONLY 可局部复验（仍初始化隔离数据与生成基础图）。不测试真实剪贴板、真实语雀账号。发布范围仅 Windows x64 与 macOS ARM64，既有 v0.4.0 正式资产不动。

## V0.5.0 上传收口与双平台 CI
- 保留语雀网页端非公开 multipart 接口，不新增其他图床。队列暂停依据 TransferProgress 的实际 HTTP 状态及远端结果分类（QueueStop），不得依赖本地化错误文案。401/403 提示检查会话与 CSRF，429 暂停，结果不明保留 unknown，不自动重传。
- 上传 JSON 解析独立测试；仅接受已有契约中的 HTTPS 链接，不把返回成功、请求体读取完成或本地入库失败混为一谈。测试使用合成响应及回环 HTTP，不访问真实语雀或真实账号。
- 正式构建矩阵仅 windows-2022 x86_64-pc-windows-msvc、macos-15 aarch64-apple-darwin；不再构建 Intel Mac 发布包。完整 CI 成功后方可发布。

## V0.5.0 舒适与紧凑模式
- Preferences.comfortable 默认 true；旧设置缺字段时使用舒适模式。布局菜单和设置 Switch 共同调用 set_comfortable；图片尺寸独立持久化。design::Spacing 统一主内容边距 24/16、图间距 16/10、日期高度 36/28、区块间距 16/8、列表内边距 16/12、导航间距 6/4（舒适/紧凑）。网格/等高/瀑布流计算与渲染必须共享 spacing；切换时重建测量、尝试恢复滚动锚点。
- 相册内容区接受 GPUI ExternalPaths 拖拽并以原生 drag_over 反馈。忙碌时明确提示未入队，不静默丢弃；保持原有持久化、去重、unknown 人工核实约束。批次进度按已处理任务计数，字节为请求体读取量，不等于远端成功；待恢复入口仅占底栏。
- 发布仅 Windows x64/macOS ARM64，最新候选双平台 CI 全通过后才下载/校验同一 SHA 的 ZIP 并发布。不可沿用旧候选资产。

## 正式版本检查
- 设置页「关于与更新」由 src/desk/updates.rs 实现，src/update.rs 独立读取固定 GitHub 仓库 releases/latest。仅用户主动点击时访问公开 API，独立无凭据客户端、12s 请求超时、1MiB 响应限制，不跟随重定向，不自动下载/安装。SemVer 比较，拒绝预发行和降级。
- 仅匹配 Windows x64/macOS ARM64 对应 ZIP 及 SHA256 文件，检查 uploaded 状态、非空大小、固定仓库下载地址；缺资产时只提供 Release 页面。手动下载、手动核对 SHA256、退出后替换，不覆盖应用或数据库，不把提供校验文件视为已完成校验。
- 测试使用合成元数据；显式运行 cargo test --locked public_release_smoke -- --ignored --nocapture 可验证公开 API，不纳入常规联网 CI。

## 应用品牌与安装包规范
- 原创品牌资源在 assets/branding，使用 scripts/generate_branding.py 生成并提交；禁止引用第三方商业品牌图标或运行时请求图标。macOS ICNS 写入 .app/Contents/Resources 和 CFBundleIconFile；Windows 使用现有锁定 embed-resource 编译 EXE 的多尺寸 ICO 和 VERSIONINFO；macOS 托盘使用透明单色模板、Windows 彩色。
- 从 V0.5.0 起正式产物仅 Windows x64 ZIP 与 macOS ARM64 DMG，均附原始 SHA256。DMG 由锁定 dmgbuild 及依赖生成固定 Finder 布局（640×420、应用左、Applications 右、真实符号链接），不用 GUI 自动化生成布局。应用 ad-hoc 签名，未 Apple 公证；不篡改系统安全设置。
- scripts/package_release.py --verify-only 先比较已有 SHA256，不重写校验文件；DMG 校验 hdiutil、实际挂载、Info.plist、ICNS、arm64、签名、Applications 链接和 DS_Store 坐标；Windows ZIP 校验 CRC、PE x64、图标与版本资源类型。更新检查优先 DMG，旧 macOS ZIP 回退，仅开放固定仓库完整资产组合。

## 存储管理与卸载安全
- src/data_management.rs + src/desk/data_panel.rs 独立管理页。统计为已识别普通文件的逻辑大小，不计外部备份；后台扫描不访问语雀。
- 轻量清理只允许 clipboard 中名称与实际 SHA256 一致、未被所有账号任何任务/本地 Demo 链接引用的普通文件；保守保护同名引用、拒绝符号链接/重解析子目录，不递归删除，不删除永久微缩图、旧版预览、用户原始文件或外部备份。所有删除均须显式确认，清理期间禁用上传。
- 保存凭据先登记账号索引；清理只处理索引和当前账号键，确认系统凭据不存在后才删除索引，失败保留索引。历史未索引孤立凭据、环境变量、浏览器 Cookie 不自动处理。删除当前账号也必须确认。
- 完全重置需备份引导与二次确认，阻止所有账号未完成/失败/待核实任务；凭据失败停止。退出辅助进程通过一次性标记和实例锁等待退出，重新核验任务/账号，只删除明确管理文件，不递归删除根目录。未知文件、实例锁、失败报告保留；非事务性删除，失败可部分完成。详见 docs/uninstall.md。
- 仅隔离测试数据；scripts/data_reset_qa.py 为 macOS QA 进程锁及重置回归，不用于真实目录。Windows 仍须 CI/真机验证。不使用真实语雀凭据或删除实际用户数据。

## V0.5.0 正式发布（2026-11-25）
- 正式 Tag v0.5.0 固定在 bdb29acc6983b248a39d7481a3904c82789d74c8；CI 45447816768 的 Windows x64 与 macOS ARM64 job 均 success。不要移动 Tag、替换正式资产或使用其他候选构建混发。
- 正式 Release https://github.com/weepwood/Yuque-Asset-Desk/releases/tag/v0.5.0 ，2026-11-24T17:27:44Z 公开，非草稿/预发行。四项资产均从指定 CI 下载，原始 SHA256、Windows ZIP CRC/PE x64/ICO 与版本资源、macOS DMG/ICNS/Info.plist/arm64/签名/Finder 布局均独立验证。
- Windows ZIP 9245630 bytes SHA256 fad393c7850ed32149c792679abccff686f3fb89ebb8ee7a72e82a2544d6d107；macOS DMG 8855946 bytes SHA256 cfa0ffd0b2bf0049ac161907adbdac46030346a4446f2e5db0b6e029e0d5f3ef。两份原始 SHA256 文件亦核对 GitHub digest；公开后四项资产重新下载并校验一致。
- 本地 QA 39 passed/1 ignored；隔离进程重置复验通过（错误 token、unknown 阻止删除、等待实例锁、外部原图/备份/未知文件保留）。CI DMG 只读挂载并复制至隔离目录实际启动，窗口 1160×828、空 Demo 数据库、托盘和快捷键注册成功；公网更新 smoke 返回 Current("0.5.0")。
- 未测试 Windows 真机、真实系统凭据批量清除、真实语雀上传、物理拖拽安装、Gatekeeper 公证或系统通知送达。macOS 启动有非致命 sandbox_extension 日志，hdiutil 有弃用提示；不修改系统权限或真实数据目录。本次仅发布与文档收口，不增加功能。

## V0.5.1 语雀账号自动识别
- 依据语雀官方 CLI 内置 OpenAPI 2.0.1 规格 https://github.com/yuque/yuque-open-cli/blob/main/spec/yuque-openapi.yaml ，GET https://www.yuque.com/api/v2/user + X-Auth-Token 仅查询个人身份。解析 data.id/name/login，明确拒绝团队或无法确认个人类型的 Token；不下载头像、不持久化 API Token。
- 独立账号页取消手填数字 ID。个人 API Token 可选；网页上传仍须 Cookie + CSRF，Cookie 中 yuque_ctoken 可自动提取，冲突则拒绝。保存必须重新查询当前输入对应身份，API Token 和 Cookie ID 不一致或任一必要查询失败不写入凭据。无 Token 仅查询既有 /api/mine。不会静默回退无效 Token。区分身份查询和上传授权，旧凭据编码/账号索引/数据库兼容。
- 两种查询分离认证头、固定官方域名、禁止重定向、12s 超时、256KiB 响应上限、错误不包含请求密钥/响应正文。仅显式按钮触发请求；成功保存清空输入框，Cookie/CSRF 保持系统密钥库。测试只用合成数据与回环 HTTP，不使用真实语雀凭据；V0.5.0 已发布 Tag 不动，V0.5.1 候选待双平台 CI。

## 文件选择窗口稳定性
- 上传及备份必须使用 GPUI 原生异步 prompt_for_paths，不再使用 rfd 或在点击事件中同步运行 AppKit modal loop。GPUI macOS 实现对 NSOpenPanel 返回 nil 有保护。
- picker_pending 阻止重复选择、上传/恢复、账号切换、数据清理及退出；取消/失败必须释放状态，不创建队列。备份选择仅允许单目录，上传仅允许文件多选。
- QA 中实际点击按钮并模拟原生选择回调验证取消/重新打开/入库/微缩图/备份。macOS 可执行 scripts/picker_native_qa.py（先 cargo build --locked --features qa），隔离 Demo 测试实际系统窗口、进程存活及 GPUI 回调；不接触用户真实文件或凭据。

## V0.5.1 舒适模式视觉密度
- design::Spacing 是唯一密度来源：舒适模式 pad/gap/day/section/row/nav=32/24/44/22/20/10，紧凑保持 16/10/28/8/12/4；footer 32/24、宽窗 sidebar 200/184、inspector 320/280，窄窗 sidebar 150。主标题 21/18、正文 14/13、图片名 12/11。绘制、虚拟布局宽度、瀑布流页脚和锚点必须共用参数；不改变已存图片尺寸。
- 舒适工具栏标题分离，Medium 按钮及响应式搜索换行；窄窗短标题，详情相关属性及账号字段标签局部归组。最小 720×600 双密度实际 GPUI 点击上传和取消回归必须通过，避免新 UI 破坏 1c25eac 异步文件选择修复。
