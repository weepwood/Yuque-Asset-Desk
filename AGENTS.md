# Yuque Asset Desk — AI 开发协作说明

## 当前交接入口（优先于后续历史记录）
- V0.9 图片上传核心修复在独立开发分支；优先实际上传、预览及恢复，具体已实现和未验证事项见末尾 V0.9 记录。正式版本仍为 v0.8.0。后续优先 UI 与操作体验，保留既有上传机制，继续 PR #9。
- 最新正式版本 v0.8.0，V0.7.0 亦已正式发布。PR #8 已合并 main 25805df，代码与固定候选一致。
- 固定发布候选：V0.7 75c173b；V0.8 aec2446。各自使用原始 CI 产物，禁止重新打包替换。详见 [V0.7验收](docs/release/v0.7.0-acceptance.md)、[V0.8验收](docs/release/v0.8.0-acceptance.md)。
- Windows 真机已调整为发布后补充验收；不可宣称完成。只处理真实缺陷和后续验收，不继续扩大功能范围；保留用户数据，中文提交，隔离 Demo 测试。
- 历史记录中的冻结状态已收口；当前 CI 仅 Windows x64 与 macOS ARM64。

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

## 原生交互动效约束
- design::SurfaceMotion 与 tween 统一使用已锁定 GPUI Kit 0.9.0 的 keyed transition/keyframes。悬停 120ms、页面/详情 180ms、预览缩放 160ms；OS reduce_motion 直接终态。页面仅淡入、详情关闭即移除、悬停退出立即取消控件命中。预览每次独立通道，拖动直接响应。
- 图库不得做全图逐项入场、宽度逐帧重排或常驻动画计时器。虚拟行与瀑布流复用可见/overscan 范围和稳定资源 ID；不要因动效新增解码、网络请求或 SQLite 查询。动画结束后不主动请求帧。维持舒适/紧凑、离线架构和异步文件选择。
- 新增 motion_samples_retarget_and_respect_reduced_motion 原生渲染测试检查中间值、快速反向连续性、完成不重播及减少动态效果。参考 PicView、Zed、GPUI Kit 仅借鉴设计原则，不复制 GPL 代码或第三方素材；详见 docs/ui-design.md。

## 面板与图片浏览交互
- 可收起侧栏保留 56px 图标导航，展开后才显示标签树；不持久化新字段。实际导航/详情宽度与虚拟图库计算统一。面板滑入只改变自身绝对位置，沉浸切换淡入；不逐帧改变图库宽度或引入常驻计时器。详情关闭立即移除。
- 图片首次点击在布局变化时先高亮再延迟 500ms 打开详情，第二次原生 click_count 可直接预览；切换资源/搜索、收起侧栏/沉浸切换取消旧任务，异步回调核对资源和历史代际。详情已打开时不延迟。系统双击间隔超过 500ms 的特殊设置未适配。
- 预览拥有独立 FocusHandle；仅预览处理左右键、+/-、0、Esc。只浏览已加载图片，跳过非图片，保持离线缩略图。滚轮锚点与拖动几何使用 album::fit_preview/clamp_pan/anchor_pan，限制可拖动范围；滚轮与拖动可中断缩放动画。preview_view 独立渲染，预览时跳过图库布局；不要将预览大渲染函数重新内联到主工作台，默认测试栈曾出现溢出。
- 设置沿用 GPUI Kit 原生菜单、Tooltip、通知及系统确认，8px 圆角，细边框分组；不替换已有破坏性操作确认或异步上传文件选择。不需要新框架或依赖。

## V0.5.1 发布冻结与候选验收
- 固定候选 efe4ef9eb8e0f44b0f1914063257ddd2555c782f，CI 45470121925。冻结业务改动；后续仅文档提交不改变发布候选。正式 Tag 必须指向完成双平台验收的源码 SHA，不能使用旧候选产物。
- 本地完整 QA 47 passed/0 failed/1 ignored，QA build、静态检查通过。隔离 macOS 实际上传与备份原生面板均为 880×448，进程存活及回调正常，未创建任务；图片浏览回归通过。独立重置安全测试通过，不使用真实凭据或用户数据。
- Windows x64 job success；首次 artifact 下载 unexpected EOF，重新下载同一 CI 后校验通过：ZIP 9283648 bytes，原始 SHA256 c5b592e962ae7c0ebf4f7aa92e14817e849379962a7d11aec8cffe5860af7e42，已核对 ZIP 完整性、PE 架构和版本资源。本地路径由 dist/release-audit-v0.5.1/verified-windows-dir.txt 记录。
- 截至本轮最后查询，macOS ARM64 job 138715498817 queued，未分配 runner，未报告代码失败。尚未取得该候选 DMG、原始 SHA256 或正式包启动证据；禁止创建 Tag/Release 直至双平台 CI 完成 success。后续下载同一 run 的 macOS artifact，verify-only、smoke_dmg.py 隔离启动，草稿核对四项资产 digest 后公开，重新下载核对并检查更新接口。不要重复触发 CI 或修改既有版本。仅 Windows x64/macOS ARM64。

- V0.5.1 CI 更新：45470121925 attempt 1 的 macOS job 138715498817 被 GitHub 取消，无 runner、无构建步骤；检查注释明确为 hosted runner 未分配及 macOS arm64 容量限制，不是代码失败。仅重跑该 job，attempt 2 已建立（macOS 138727948374 queued，Windows 保留 success），候选 efe4ef9 不变，继续复用原始 Windows ZIP/sha256。证据位于 dist/release-audit-v0.5.1/。不重复触发完整工作流；通过 macOS CI、DMG 校验及正式启动后才发布。

## V0.5.1 正式发布（2026-11-25）
- Tag v0.5.1 固定 efe4ef9eb8e0f44b0f1914063257ddd2555c782f，CI 45470121925 attempt 2 双平台 success；正式 Release https://github.com/weepwood/Yuque-Asset-Desk/releases/tag/v0.5.1 ，2026-11-24T19:55:48Z 公开。不得移动 Tag 或替换资产。
- Windows ZIP 9283648 bytes SHA256 c5b592e962ae7c0ebf4f7aa92e14817e849379962a7d11aec8cffe5860af7e42；macOS DMG 8887805 bytes SHA256 4b1bea16c7f49837561d503dac202f0f5067f9ac5339dfca04dbaa09509484b7。原始校验文件和 GitHub 四项资产 digest 一致，公开后重新下载验证通过。更新检查 Current("0.5.1")。
- macOS CI 正式 DMG 只读挂载/复制/签名/架构/布局校验，隔离实际启动窗口1160×828、托盘正常；快捷键已被占用，正常降级 hotkey=false，不声称注册通过。未安装至 Applications、未使用真实凭据；Windows 真机、Apple 公证、真实上传未验收。证据 dist/release-audit-v0.5.1/。

## V0.6.0 按需原图预览专项
- V0.5.1 正式 Tag efe4ef9 不改变；独立 feat/v0.6-preview 分支、单 PR。src/high_preview.rs 复用 image/reqwest/GPUI RenderImage，只有显式操作才读取本地原图或在线请求。默认永久离线 160px JPEG 不改；不新增磁盘缓存或依赖。
- 本地路径来自 Demo file URL/当前资源所属账号的已完成任务记录，普通文件、32MiB、SHA256 严格验证。线上仅 yuque provider 的 https://cdn.nlark.com/yuque/ (443、无 URL 用户名密码)，遵循 TLS/系统代理，不跟随重定向、不传 Cookie/API/CSRF/Referer；HTTP 200、大小和 SHA256 均检查。CDN 不一致或其他链接使用浏览器；不自动请求或回退到网络。1600万像素、8192单边、96MiB image decoder alloc limit，最长边2560显示，PNG/JPEG/WebP/GIF首帧/BMP，不支持矢量或动图播放。
- 当前高清图只保留单个 RenderImage（最长边2560，GPU/CPU 实际峰值未测）；切换/关闭/筛选重置 cx.drop_image；generation 和 asset id 拒绝过期回调；单工作锁防并发解码，取消后后台请求可能继续直至超时。保留低清图供失败回退，不写源文件或永久微缩图。
- 预览邻近七张缩略图仅使用本地缓存；左右键及原生按钮跨页追加（沿用当前账号/筛选/排序/收藏标签查询，最多九页每次，非图片跳过）。退出或切换取消待跳转。复用 existing page cursor/virtual gallery，不做全量加载。
- 验收：51 passed/0 failed/1 ignored QA、QA build、静态检查、diff check；真实 GPUI 点击本地原图及缩略图、跨150条边界跳过附件并返回、高清释放、缩放/拖动/退出；回环 HTTP 200/302/403/超限、无认证头；源文件 hash/尺寸/BGRA/下采样/不写预览测试。7张实际渲染截图含高清深/浅/窄及离线；四张预览截图已审查。未测试真实 CDN、Windows 真机或 Release 性能，不提前发布。

## V0.6.0 预览专项可靠性收口
- 高清来源切换：当前同源已加载时不重复读取；切换其他来源失败保留原高清图，成功后 drop_image 旧图；显式“离线预览”释放当前高清并使后台回调过期。稳态仅一张高清，切换成功前可能短暂同时存在两张；阻塞 HTTP 尚不能立即中止，单工作锁及20s请求超时继续保留。
- 预览导航及本地缩略图生成共用 preview_strip 的邻近七图范围，靠近末尾向前补足，首尾均不超七；跨页没有缓存时使用既有本地恢复机制，禁止网络缩略图。恢复选中图片时同步写入内存路径/宽高比，返回详情可复用。
- 上传结束等 reload 请求在预览期间合并为一个待刷新标记；返回相册再执行一次 reload，当前页游标、选中图片及左右浏览不被异步刷新打断。显式筛选/账号 reset_history 仍立即清理旧状态。预览单独显示图源、尺寸和失败/加载状态；打开原文件失败可在预览内看见。
- 本地51项完整QA、QA build、cargo check --locked --all-targets、静态检查、diff check通过；既有两项实际GPUI预览回归增加替代来源失败保留、离线释放、跨页真实本地源恢复/比例、刷新延迟与返回刷新断言。深浅窄窗高清及离线截图重新生成；未使用真实语雀CDN、未做Windows真机或正式性能基准。保留 PR #3，等待新提交双平台CI，不创建新版本Tag。

## UI 设计规范（V0.7）
修改任何 UI 前必须阅读根目录 DESIGN.md 与 docs/design-references.md。历史 docs/ui-design.md 只作变更记录；以真实参考规范、操作契约、实际截图和 GPUI Kit 0.9.0 API 为准。不得以堆叠边框、留白或动画替代工作流设计。保留用户数据、异步文件选择及离线架构。
- V0.7 后续迭代入口：docs/development-status.md 的「V0.7 设计重构第一批」。PR 未成功时先收口该 PR；成功并审查后再独立分支推进剩余设计/架构差距。无实际证据不得声称验收完成。

## V0.7 交互核心第二批
- PR #4 已合并 main 5782dd0（原 PR CI 45480921070、main CI 45532340902 均 success）。历史 #1/#2 已关闭并保留分支；GPUI 迁移/原生拖放/持久队列/多选 Markdown 已替代旧实现，专用本批链接入口未原样保留。当前 feat/v0.7-interaction-core，正式 Tag 仍 v0.5.1；Cargo 版本暂 0.6.0，不提前发布。
- ViewerModel 独立 Entity 拥有高清任务/代际/asset id/RenderImage，clear 丢弃过期结果并 drop_image；observer 仅新高清成功时复位缩放。InspectorModel 标签错误及重试。Gallery/Queue/Account 仍内部状态，不声称全量 Entity 化。
- mutations.rs 单资源写入通道：收藏、单项标签、批量标签离开 UI 线程，持有 task 直至完成；resource_write_busy 在账号、上传入口、备份、清理和退出路径检查。不可把这些异步写入 detach 后允许重置。队列、账号索引、偏好等仍有同步 I/O，需后续按实际锁与生命周期迁移。
- 原生 Gallery Action/context 仅图库焦点路径，方向键按 row/masonry 几何，Shift/Cmd/Ctrl 与多选预览返回统一；显式详情命令一致。独立 settings_view，顶部账号入口、固定设置分组、单一状态栏，视频/附件图标、多选详情。图片仍完整 contain，不裁剪原始内容。
- inspector-slot 保留退出空间，180ms keyed tween，关闭时禁用所有内部控件且拦截鼠标/键盘，结束释放布局；不逐帧重排图库。快速反向和 reduce_motion 用真实渲染测试验证；侧栏/沉浸完整退出未完成。
- 55 passed/0 failed/1 ignored，all-targets check、QA/debug及release QA build、static/diff check；SQLite BEGIN IMMEDIATE 真实写锁测试 UI dispatch <1s 并验证门禁/最终持久化；多选预览恢复、几何和实际面板反向。15张真实截图（含720×600本地原图与源缺失回退、深浅/账号/设置/详情/任务）；8步原生注入截图与状态 trace，ignored dist/design-research/core-after/、path/operation-path.gif。Release 10000合成JPEG 各120 CPU draw 样本 grid p50 2.199 p95 5.590ms RSS148.12MiB，masonry 4.550/6.332ms RSS142.16MiB，300 loaded/cache192或191。非实际显示FPS；未与同版本相同工况作受控对照，不宣称性能提升。证据 docs/performance/v0.7-interaction-release.json 与 v0.7-interaction-path.log。
- 后续：真正 Gallery/Queue/Account Entity、队列/账号/偏好异步写入与清理互锁、面板退出焦点与全菜单路径、侧栏/沉浸完整动画、真实设备及性能回归、正式发布。禁止把部分落实表述为全部完成。先验收本 PR，勿扩展新业务。

## V0.7 I/O 生命周期第三批
- PR #5 ee6863b 双平台 CI 45533782259 success，合并 main abc943e；main CI 45535347941 success。新分支 feat/v0.7-io-lifecycle。保持正式 v0.5.1 Tag，Cargo 0.6.0。
- JobHistory 独立 Entity：账号 scope、代际、50 条分页、异步任务列表及 SQL COUNT 可恢复计数；切换账号清空旧页并拒绝旧结果。队列执行和账号全量 Entity 化仍待处理。
- queue_prepare.rs 后台入队、重试、恢复、核实与剪贴板暂存；busy/preparing 持有 task，准备到执行间不得插入账号/资源写入/备份/清理/退出；准备状态不显示可暂停按钮。剪贴板 hash 匹配才复用，create_new 暂存并 hard_link 无覆盖发布，同目录文件系统需支持硬链接（不支持则安全报错，不改变原始文件）。任务完成计数走异步 history，不再同步拉取全部可恢复记录。
- preference_writer.rs 按 revision 串行写快照，180ms debounce/coalesce，不取消已启动写入；旧快照不得覆盖已提交新快照。正常关闭先检查 busy，写入尚未结束时拒绝退出；最终 flush 同步兜底（系统强制退出仍有等待风险），不声称全生命周期零同步 I/O。重置 pending/armed 阻止启动偏好写入，清理/重置门禁等待 prefs_writing=false。
- 账号验证后索引保存与列表读取后台执行，仍先索引后系统密钥库；凭据删除后索引删除与重读后台执行，保留失败索引。账号启动读取、重置预检查/调度及部分维护写入仍同步；未使用真实凭据。
- 59 QA pass/0 fail/1 ignored、all-targets check、QA build、static/diff；实际 SQLite BEGIN IMMEDIATE 下入队与设置更新 UI dispatch <1s，重复入队拦截，仅一个 done，最新 tile204；独立 history scope 切换和51条分页、剪贴板校验及不覆盖。真实 macOS upload/backup NSOpenPanel 880×448，进程存活和窗口内回调，jobs=0（不是物理点选）；隔离 reset helper 验证错误 token/unknown任务/实例锁/受控删除及原文件备份保留。8 张不同界面截图 dist/design-research/io-after/（合成资料）；任务计数改为“可恢复”，避免与所有未完成混淆。证据 /tmp/yuque-v07-io-*.log。
- 后续集中推进 Gallery/Queue runner/Account 真正 Entity 化、余下同步 I/O、侧栏与沉浸动效、同条件 Release 性能复测、真实 Windows/macOS 包验收。不重复无必要全量基准，不新增业务功能。

## V0.7 运行边界第四批
- PR #6 固定 54b3831，双平台 CI 45536097492 success，合并 main c2bb22d，main CI 45540060853 success。第一次合并后 git fetch HTTPS 失败，随后已核实 MERGED 并成功 fetch。独立 feat/v0.7-runtime-boundaries 基于同内容的 54b3831；正式 Tag 保持 v0.5.1，候选 Cargo/Cargo.lock 0.7.0。
- QueueRunner retained Entity 拥有 prepare/upload/progress task、状态、控制；shell 订阅类型化事件，账号快照固定到批次，完成/取消/暂停统一触发图库刷新一次。保留串行协议、远端结果未知门禁、每文件任务状态；暂停/取消期间累计状态写入失败不可被最后一项成功覆盖。
- GalleryQuery retained Entity 持有查询与 generation，过期请求不发结果；shell 仍负责当前结果合并、选择、布局、缓存及预览协调，不声称所有 Gallery 状态已独立。AccountModel retained Entity 持有凭据表单、识别/会话/vault任务和索引任务代际；shell 持有已激活 Config 和独立 maintenance_busy，避免数据维护占用账号模型状态。
- 启动偏好在 application.run 前读取一次用于窗口初始尺寸/主题，账号索引后台加载；重置 arm 与确认后 preflight、账号索引及 reset helper 启动后台执行，确认前后安全门禁、实例锁及源文件保护不变。普通退出先核对最新偏好保存成功，失败/未完成拒绝退出并可重试；托盘隐藏不阻塞写入；系统外部直接退出的后台最终保存受 GPUI 200ms shutdown timeout 限制。启动数据库创建/打开仍是窗口启动前同步 I/O；部分剪贴板读取和平台 API 必须 UI 线程，不声称绝对无阻塞。
- 任务中心对齐列，账号当前状态顶部紧凑行、Token 同行识别、Cookie/CSRF 连续分组；等宽图库缩略图底部对齐、不裁剪。侧栏收起与沉浸退出保留空间至180ms过渡终态，图标栏可反向，沉浸退出的控件拦截输入、进入恢复图库焦点；展开预留最终宽度，不逐帧 reflow，减少动态直接终态。退出布局不是连续宽度动画。
- 61 QA pass/0 fail/1 ignored、all-targets、QA build、static/diff；三文件批次仅一次图库刷新、侧栏/沉浸真实渲染反向和减动效、锁竞争后退出条件回归；15张不同界面截图（两次采集，账号与设置最后以0.7.0构建复验），dist/design-research/runtime-after/；8阶段实际 GPUI 注入轨迹 docs/performance/v0.7-runtime-path.log；macOS 上传/备份选择窗进程存活与回调、隔离 reset helper 通过（无物理点选/真实凭据）。未改用户数据、原文件、离线微缩图、数据库schema或第三方依赖。
- 发布前必须确认新 PR 双平台CI与产物、Windows真机、macOS正式DMG、实际凭据/语雀测试的授权边界以及性能证据；候选未正式发布。不可将 Gallery 全部状态、所有平台强制退出及所有视觉路径表述为完全验收。
- 本批补充 Release A/B：固定 54b3831 独立 worktree 与0.7.0候选顺序构建/同脚本万级合成数据，网格 P50/P95 2.103/5.011→1.200/2.260ms RSS149.55→133.55MiB，瀑布流2.624/4.732→1.456/1.775ms RSS147.16→133.39MiB；最终600/450条一致、各120采样、缓存192、无超时。原始报告 docs/performance/v0.7-runtime-{baseline,candidate}.json 和 comparison.md。单次同条件对照不是受控统计或FPS。

## V0.7.0 发布收口（固定候选）
- PR #7 候选 75c173bf0630d6eed2cdcb07d1a6fa3f6e4de996，CI 45541767866 双平台成功，合并 main b58ee1b；合并树与候选完全一致。冻结功能，当前正式版本仍 v0.5.1。
- 原始 CI Windows x64 ZIP 9422058 bytes / macOS ARM64 DMG 9026105 bytes 与两份原始 SHA256 均验收，保存 dist/release-audit-v0.7.0/ci-artifacts/，四项摘要 docs/release/v0.7.0-verified-assets.json；Windows 校验文件 CRLF 导致 Mac shasum -c 路径误判，Python 独立验证及 verify-only 均通过，未改写原始文件。
- macOS 正式候选 DMG 只读挂载、ad-hoc 签名、真实隔离启动、1160×828 窗口、空 Demo DB、tray/hotkey 注册均通过；非阻塞 sandbox_extension 警告。无 Gatekeeper 公证/真实账号/物理点选。
- Windows CI 单测、Release、PE/ZIP 与版本元数据成功；Desktop Commander Windows 设备离线，尚未真机交互验收。按前轮发布门禁暂不创建 Tag/Release，保留原始产物、RELEASE-NOTES.md 和详细验收记录 docs/release/v0.7.0-acceptance.md。完成剩余验收（或用户明确调整门禁）后固定原候选、发布四项资产、重新下载验证与公开更新接口；不重跑已通过的全量本地测试，不扩大功能。

## 后续设计深化：浏览上下文（V0.7候选冻结）
- V0.7 固定候选75c173b与原始四资产不变，合并后main CI 45542564057 双平台成功，仍待Windows真机或明确调整门禁；未创建Tag/Release。新分支 feat/v0.8-browse-context 基于文档提交5b0c33c，不混入冻结产物；Cargo仍0.7.0，非新发布。
- 重读锁定SpaceUI Explorer与GPUI Design Guides，新增DESIGN.md第11节。普通浏览统一账号范围/标签/组合收藏/搜索条件行；清除恢复当前账号全部资源并同步搜索输入。分类高亮独立于标签及组合收藏；标签仅真实上下文高亮，收起侧栏保留账号入口。
- 相似查询按源资源所属账号及相似度，保留原搜索/标签/收藏/日期偏好；普通搜索及筛选禁用，排序与日期命令不显示，显式返回恢复原组合。最多150项；普通分类入口离开相似上下文。相似模式保留原生布局菜单。
- SQL原total为账号总资源量，不进行额外筛选COUNT：有过滤且分页未完显示已加载数，最终显示实际结果数，读取中/失败不显示陈旧总数。空结果可清除/返回；不变更SQLite schema/上传/原图/永久微缩图。
- 62 QA通过/0失败/1忽略；新增720×600真实GPUI指针与键盘路径，验证组合条件、相似结果返回、搜索输入、空结果清除、1项/0项计数。all-targets check（计数精修前）、最终QA build及static/diff通过。8种实际截图，3张上下文图最终重新捕获，修复截图发现的陈旧计数与分类高亮；保存dist/design-research/browse-after/。无真实用户数据、Windows真机、全量Release性能复测；下一批优先基于实际操作痛点检查查看器/任务反馈，而非无目的视觉修改。

## 后续设计：核心操作体验第二批
- PR #8 的934ef7a双平台CI45544055365 success；不合入冻结V0.7主线，直接追加同一PR集中交付。固定V0.7候选75c173b、原始四项资产、正式v0.5.1均不变。
- 重读固定Immich AssetViewerNavBar/DetailPanel及GPUI Design Guides，DESIGN.md第12节：查看器显示适应窗口相对缩放、当前/已加载图片序号(+有分页)，缩略图完整contain与当前标识；区分无图/读取/离线/高清，“停止等待”仅取消结果接收并回离线，不保证网络立即终止。
- QueueRunner.activity_label/transfer_label共享真实请求状态，准备/暂停/取消/中止/请求体读完等待结果，批次和单文件反馈分层。中止使用fetch_max(1)，不能将已请求取消2降级为暂停1。重复命令禁用；当前文件不显示继续按钮。远端待核实操作新增原生确认，maintenance_busy持有至确认结果，取消不改变状态、确认仅Acknowledge且不自动上传，SQL继续核对当前账号和原状态。
- 详情对象名称/本地化类型置顶，复制链接主按钮，打开/Markdown/收藏/相似动作优先，标签右侧保存；窄窗144px预览及明确返回图库。保留低频技术属性、原图和永久微缩图；无新增依赖/schema/网络功能。
- 64 QA passed/0 failed/1 ignored，all-targets、QA build、static/diff；新增命令优先级及请求体读完不等于成功回归、待核实原生确认平台模拟回调（取消保留unknown、确认failed、未上传、互锁退出/重试）。11种不同真实渲染截图（两轮重点复验，深浅及720×600；活跃任务是隔离QA状态夹具非真实上传）；dist/design-research/flow-after/。最终查看器/详情和活跃任务最小窗口已实际审查。未执行真实语雀、Windows真机、完整Release性能复测、完整物理交互；继续保留发布门禁。

## 后续设计：图库选择与读取恢复
- PR #8 的 f0d9f27 双平台 CI 45546108736 成功，冻结 V0.7 候选及其原始产物不变。继续在同一后续开发分支提交，不提前合入 main 或发布。
- selection.rs 双向范围超过150项时保留点击端点附近连续150项，追加选择不驱逐已有项；工具栏显示上限。SelectAll 同步主选项锚点。resources.rs 共用 clear_resource_selection，取消选择、查询重置和整页读取失败统一释放详情/预览状态；主选项刷新后消失则保守清空整个选择，追加页失败保留既有结果及选择，重试游标回退。
- 本地 macOS all-targets check、static/diff check、范围专项及 GPUI 状态注入刷新失败/追加失败/恢复测试通过；完整 QA 65 passed/0 failed/1 ignored。未重新采集截图、运行真实 Windows、真实语雀或 Release 性能。未新增依赖、迁移、网络或用户数据操作。

## 后续设计：图库查询取消与相似检索成本
- 基于 PR #8 d09e6b3，前一候选 CI 45546796255 查询时仍在运行；继续同一分支，保持 V0.7 固定候选75c173b与正式v0.5.1不变。
- GalleryQuery 新请求和 Entity Drop 发出协作式取消；仍通过 generation 拒绝过期结果。普通非追加搜索100ms debounce；追加/相似不等待。后台在分批读取、哈希、写入、匹配、元数据阶段检查取消，不中断已进入的 SQLite/文件/解码操作，已提交的派生索引保留。
- Storage::unindexed_images 使用源账号和 keyset 分页，SQL NOT EXISTS 排除已有哈希，不做每页 COUNT 或每候选独立哈希查询；缺缩略图继续跳过。similar_assets_until 保留最多150个排名结果（距离升序、ID降序），支持扫描内取消；相似结果收藏与批量标签以源账号为准。
- 本地 all-targets/static/diff 与完整QA 67 passed/0 failed/1 ignored。新增预取消测试、500项真实SQLite数据分页/跨账号/200项已索引候选的有界排序及扫描中止测试。首次编译发现 GPUI test 属性名冲突，改用明确 Rust 内建属性后通过。未做真实磁盘压力、全量Release性能或视觉回归，不宣称量化性能提升；无新增依赖/schema/网络或真实用户数据操作。

## 后续设计：缩略图生命周期与缓存释放
- 基于 PR #8 1e723b8，CI 45547161999 最近查询仍在运行。继续同一分支，冻结 V0.7 候选75c173b、原始产物及正式v0.5.1不变。
- ThumbnailModel 独立 retained Entity 管理12项批次、协作式取消、代际及完成尝试；分页追加不取消、不重试已失败项；整页刷新重置，离开图库暂停，返回恢复，迟到事件二次核对代际。取消后当前单项I/O仍可能完成并保留派生文件。图库展示/选择/布局和路径/比例映射仍由shell协调。
- 详情复用已知本地预览路径，恢复时更新比例；账号切换调用 AlbumImageCache::clear 与原生 drop_image，保持192项LRU上限。缓存清理不触及永久微缩图和用户原文件。可视范围异常边界防护。
- 本地 all-targets、static/diff、QA build、68 QA passed/0 failed/1 ignored。新增151项真实隔离SQLite与本地图片回归：分页保留失败记录、设置暂停、返回可恢复、2:1比例、详情复用、实际GPUI渲染缓存账号切换清零，原图及JPEG保留。7张实际渲染截图保存dist/design-research/thumbnail-after/，已查看浅色图库、720×600详情和离线查看器；不是实际用户素材。未做Windows真机、Release性能、长期内存测量或真实语雀；不新增依赖/schema/网络。

## 后续设计：原生详情焦点与 Release 性能复验
- PR #8 b00471f 的 CI 45547718069 成功且无合并冲突；继续同一分支，不改 main/冻结候选75c173b及正式v0.5.1。
- 固定详情头部并独立滚动正文；统一空/多选/单选返回。独立 inspector_focus 与 YuqueInspector 关闭动作，窄窗打开/查看器返回聚焦详情，Esc/Ctrl/Cmd+I 关闭还焦图库；显式分类/标签/相似/恢复入口返回窄窗图库；窗口缩窄修正焦点，退出过渡不注册焦点路径。宽窗普通切换维持图库焦点。
- 780×640原生GPUI测试增加滚动头部固定、Esc、Ctrl+I、预览返回及分类恢复路径，完整QA 68通过/0失败/1忽略，all-targets、QA debug/release build、static/diff通过。8张实际截图在dist/design-research/focus-after/，审查深浅详情和720×600详情。
- 同一10k合成JPEG Release QA脚本复测：第一轮与截图任务重叠，CPU p95 grid/masonry 4.829/4.295ms、峰值RSS147.86/148.25MiB；独立重跑为2.064/1.577ms、138.47/143.14MiB，旧基线2.260/1.775ms、133.55/133.39MiB。重跑首屏103/100ms（旧142/115），加载600/450项，解码均192/192，无超时。三份JSON保存在上述目录，详见development-status。单次短测非硬件FPS，不能证明长期内存稳定或排除环境噪声；RSS较旧基线仍高，下一轮优先长程滚动、账号切换缓存/内存与性能定位。Windows设备离线，未做真实语雀、物理设备交互；没有新增依赖/schema。

## 后续设计：原生纹理回收与长程滚动诊断
- PR #8 84bab5e 的 CI 45548753872 两平台通过。继续同一分支，不改main/冻结V0.7候选75c173b及正式v0.5.1。
- 依据GPUI 0.3.11 App::update_window_erased/drop_image源码，批量清理与高清图替换/关闭使用App::defer，在窗口回到窗口映射后释放纹理。离开图库释放解码缓存，返回重新解码；保留资源/滚动位置及永久JPEG。重置释放旧容器分配，清理滚动回调；弱引用布局及身份校验拦截旧事件。仍使用192项LRU。
- 隔离QA 68通过/0失败/1忽略，all-targets、Release QA build、static/diff、Python语法检查通过；初次QA编译的AnyView调用错误已改为显式Entity读写，最终通过。脚本YUQUE_PERF_MEMORY=1 python3 scripts/album_perf.py，10k合成JPEG、每种布局3轮180次原生滚动+离开/返回/演示账号重置，共1080次滚动和54个检查点，全部门禁通过。网格每轮1050项、峰值140.16MiB，瀑布流2250项、峰值155.42MiB；离开6次缓存均0，重置6次均150项。报告dist/design-research/memory-after/。
- 两模式各约22秒，非数小时耐久测试；RSS最近采样值不等于GPU专用内存，未独占机器，无旧版本同长程基线；不能证明没有泄漏。真实账号切换、Windows真机仍未验证。后续优先精简布局全量重算与视觉收口，按风险分级；Desktop Commander提示本月使用量达到80%。

## 后续设计：瀑布流增量与裁剪
- 本地9594ac9之前TLS推送失败，本轮成功推送并核实PR #8 head；CI 45549944690查询时运行中。继续同一分支，冻结V0.7候选75c173b及正式v0.5.1不变。
- MasonryPlan 保存列底部、日期边界与实际最高磁贴，普通追加仅计算后缀，整页替换/已有比例变化/几何变化全量重排。真实高度裁剪、日期二分定位、滚动锚点限域，行键盘导航避免为前置行分配中心数组。
- 新增10k资源、150项分批、双密度/日期模式增量与全量逐磁贴/日期/高度等价及可见区暴力对照测试。首次测试490px紧凑模式正好两列，超高夹具断言失败；改480px覆盖两种密度单列超高磁贴后通过。完整QA 69通过/0失败/1忽略、all-targets、Debug QA build、static/diff通过；实际780×640瀑布流截图已审查，保存在dist/design-research/layout-after/。未重新测Release/长程RSS，不宣称量化收益；未改依赖、数据库、真实数据。剩余视觉与网格布局优化按实际收益收口，Windows真机门禁不变。

## 后续设计：行布局增量与分页反馈
- 0dac367 的TLS推送阻塞已解除，远程PR #8 head核实一致；前一9594ac9 CI 45549944690双平台通过，0dac367 CI 45551785937查询时运行中。保持V0.7固定候选75c173b、正式v0.5.1与main不变。
- 网格/等高拼图/列表分页只重排末行及后缀；保留首段日期标题、GPUI ListState splice只替换变化后缀。普通追加不增加既有比例revision，旧比例更新/几何变更/完整替换重算。渲染读取当前行计划，滚动回调弱引用身份校验；NaN等无效比例回退。固定底栏展示追加加载/错误，避免正文插入提示改变视口高度。
- 新增10k、150项分页、三布局×双密度×日期开关等价测试，覆盖跨日期、跨页及异常比例；完整QA 70通过/0失败/1忽略，all-targets、QA Debug构建、static/diff通过。真实深色拼图和浅色网格截图审查，保存在dist/design-research/row-after/。未重复Release/RSS，不量化端到端收益；未单独测加载态像素位移、真实Windows/语雀。无依赖/schema/真实数据改动。下一轮优先集中收口、复查CI及剩余真实视觉问题，不扩张功能。

## V0.8.0 候选集中收口
- ef31fce CI 45552122688 success；窄窗分页状态栏优先错误与重试，新增真实GPUI几何不变及重试点击回归，完整QA 71通过/0失败/1忽略，全目标编译、Release QA、static/diff通过。Cargo.toml/Cargo.lock仅修改自身版本0.8.0，无依赖或schema变化。
- 11张实际Release截图生成，人工审查最小窗口图库/任务/设置/详情4张。顺序执行10k合成JPEG性能基准，均未超时，192缓存上限；完整指标及剩余门禁见docs/release/v0.8.0-acceptance.md。报告与截图位于dist/design-research/v0.8-closeout/。不宣称真实用户上传、Windows真机、全量视觉审查或长期内存验收。
- 当前阶段停止扩大开发范围，待本提交CI及最终候选原始产物核验；仍保持V0.7固定候选、main及正式v0.5.1不变。仅修复发布实际阻塞。

## V0.8.0 固定候选验收

## 最终固定候选与原始 CI 产物（2026-11-25）

- 固定代码候选：aec2446fa46c172840f38066e87e564c6422799e；PR #8，CI 45553650482（headSha 一致，双平台 success）。之后仅允许验收文档更新，不改变此候选的源代码和原始产物。
- 原始 Windows x64 ZIP 9439257 bytes、macOS ARM64 DMG 9040235 bytes，及各自原始 SHA256 文件均从该次 CI 下载，保存于 dist/release-audit-v0.8.0/ci-artifacts/；四项完整摘要见 [v0.8.0-verified-assets.json](v0.8.0-verified-assets.json)。校验前后均比对原始摘要，不重新打包或覆盖校验文件。V0.7 四项原始文件也重新核对，保持不变。
- Windows：ZIP CRC、x64 PE、图标/版本资源、INSTALL.txt 和原始 SHA256 通过；CI 原生 ProductName/ProductVersion 检查通过。实际 Windows 设备仍离线，不宣称运行、托盘或文件选择验收通过。
- macOS：复用 dist/dmg-venv，原始 DMG CRC、只读挂载、ARM64、应用版本/图标/标识、ad-hoc 签名、Finder 安装布局通过。隔离复制后实际启动，Demo SQLite 初始化为空、1160×828 窗口、DESKTOP tray=true hotkey=true；测试进程已退出。默认 Python 缺少 ds_store，切换既有打包虚拟环境后通过；hdiutil 弃用及 sandbox_extension 警告不阻塞本次启动。未安装到 Applications、未验证 Gatekeeper 公证、未触发物理快捷键/真实上传。
- 完整验收日志与 smoke.json 保存在 dist/release-audit-v0.8.0/。正式版本仍为 v0.5.1。V0.7 固定候选优先收口，Windows 真机门禁未完成，不合并 PR #8，不创建 Tag/Release，不改变发布顺序。

## 正式发布收口（2026-11-25）
- 根据本轮对简化发布方案的确认，Windows 真机调整为发布后补充验收，未虚报实际设备运行。V0.7.0 正式 Tag/Release 指向 75c173bf0630d6eed2cdcb07d1a6fa3f6e4de996；V0.8.0 正式 Tag/Release 指向 aec2446fa46c172840f38066e87e564c6422799e。两次正式发布前四项远程草稿资产的 SHA256/大小与原始 CI 文件完全一致，未重新打包。
- PR #8 已合并 main 25805df371df4b1b258e184e61a2908c272e60b7；合并后除 AGENTS.md 与 docs 外，文件与 V0.8 固定代码候选一致。使用已通过的候选 CI 45553650482 与文档提交 CI 45555047033，不以新 main 构建替换发布资产。
- 最新正式版本 v0.8.0；保留 v0.7.0 与 v0.5.1 历史版本。未公证 Mac、未签名 Windows、未进行真实语雀上传，Windows 真机、物理托盘/快捷键及文件选择为剩余补充验收。历史冻结记录仅作为当时证据，本节覆盖旧发布状态。


## V0.9 图片上传核心业务修复（第一批集中实施）
- 基线 main 45d399f / 正式 v0.8.0，分支 fix/v0.9-upload-pipeline。V0.8 正式发布和原始产物不变，Cargo 自身版本调整为 0.9.0 开发候选。新增直接依赖 resvg =0.46.0（复用已有锁定版本，禁止外部/嵌入位图资源；跨平台 CI 待验证）。
- 增量建表 asset_sources、asset_preview_state、upload_job_assets、upload_receipts；保留旧表、去重约束、原始数据和旧缓存。上传内容使用有界私有快照；来源、任务 done 与资源同事务。已写入的远端回执重启可恢复，完全不可写时仍需人工核实，不能重传。
- 内容识别、SVG、有界位图解码、缓存内容校验、错误持久化、主动 CDN 补图、远端转码显示、首屏单张完成刷新、任务中心预览状态、暂存空间统计及退出重置。远端 CDN 恢复仅显式用户操作，不带账号凭据，不自动跟随跳转。现阶段上传/预览仍串行处理；不是完整独立后台任务系统。
- 本地 cargo check --locked --all-targets、static/diff、cargo test --locked --features qa 全部通过；74 passed/0 failed/1 ignored。新增真实 PNG+localhost multipart HTTP 响应夹具测试（核对参数、内容 MIME、文件名、传输字节，模拟原图修改、重启、重复来源及损坏缓存恢复）、远端回执重启与预览失败独立测试、SVG/超长图片/模拟转码缓存恢复。只使用隔离数据库、合成图与虚构会话，未访问真实语雀账号或输出真实凭据。
- 剩余：真实语雀接口/响应契约及授权素材端到端、AVIF/HEIC原生跨平台解码、上传前确认预览、独立预览工作队列、已有失效远端链接验证及全量来源迁移、真实视觉/大型图库性能复验、Windows真机。新增表属于兼容迁移，需备份恢复与旧版迁移回归；不宣称本轮已完成全部八项或正式发布。


## V0.9 原生 UI 集中深化（PR #9）
- 基于 bd197bb，上一轮 CI 45564536201 双平台 success。设计参考：Eagle https://en.eagle.cool/ 的资源分类与快速浏览；Apple Photos https://support.apple.com/guide/photos/browse-your-photo-library-pht53854a251/mac 的宽高比与原图浏览；Zed https://zed.dev/docs/visual-customization 的工作区面板与视觉定制。借鉴交互原则，不复制商业资产或源码。
- 保留 GPUI Kit 0.9、语雀绿、深浅双主题与现有动效，统一中性画布、侧栏、承载表面、分隔线及7px圆角。图库图片居中呈现、区分等待/缺失预览、悬浮复制链接；右侧检查器在宽窗可拖动调整240–420px（随可用宽度约束），释放保存偏好，窄窗继续全宽详情。详情补充图片状态和扩展名格式；查看器采用独立低干扰画布、缩略图条。
- 文件选择、拖拽及剪贴板统一进入上传确认页；后台预检文件大小及前24项有界内存缩略图，确认前不入库、不发请求；无效文件明确提示并跳过，最多展示100项，确认数量包含全部有效文件。取消或账号切换释放预览；后台结果受代际约束。确认后沿用V0.9快照/回执/事务和队列机制。上传前预览不是上传内容承诺，确认后仍由正式上传流程重新校验原件。
- 任务中心增加添加文件、完成资源缩略图、上传与预览状态区分及错误展示，图库底部汇总支持回到待确认批次；正在上传时保持原有单文件读入进度与暂停/取消语义。设置页面明确分组说明。
- 真实 GPUI Debug QA：76 passed/0 failed/1 ignored，all-targets/static/diff及QA build通过。旧文件选择、拖拽、粘贴用例调整为先验证未入队，再确认上传；新增720×600确认/有效性/账号切换与1360×800检查器拖动持久化测试。15张实际GPUI截图保存 dist/design-research/v0.9-ui/，人工查看确认页深浅、图库深色、详情宽/窄、任务深色/最小窗口、最小设置、深色高清查看器9张；其他已生成未逐张审查。不声称正式Release构建或跨平台真实视觉验收。
- 未改变数据库结构或依赖，不读取真实Cookie、不发真实上传，不改变V0.8正式版本。下一轮继续以UI为主：提升确认页单图大预览/剔除与连续添加、底部队列常驻和历史状态操作、侧栏自由调整与面板动效，完善错误及空态、三种分辨率双主题视觉验收；真实语雀上传仍需授权素材。继续集中PR #9，尚不合并发布。
