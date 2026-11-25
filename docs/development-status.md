# 开发交接：V0.4 开发阶段（尚未发布）

## 2026-11-24 本地实施
- Rust 1.100.0 / Cargo 1.100.0 已安装；Cargo.lock 纳入版本控制；CI 使用 --locked。
- 保留原 assets 表及原数据目录，增量创建账号索引、任务表、标签表和组合索引。
- 历史查询默认隔离账号；可显式查看全部；50 条游标分页、正反向时间顺序、名称/标签搜索；后台查询按代次丢弃过期结果。
- 三栏资源工作台、亮暗主题、列表/网格、详情、标签编辑及当前页批量复制 Markdown。
- GPUI 原生密钥库 API 保存/读取/删除凭据；SQLite 只保存账号 ID，不保存 Cookie 或 Token。配置成功不等于会话验证成功。
- 原生文件拖放、系统文件选择、剪贴板文件/图片导入；剪贴板图片源文件保留在数据目录以支持恢复。
- SQLite 持久化任务、手动恢复、文件边界暂停/取消；语雀文件之间默认间隔 350ms（YUQUE_UPLOAD_INTERVAL_MS 可调至 0~30000），401/403/429 暂停后续任务；不自动重试结果不确定的上传。单实例文件锁避免同一数据目录并发运行。
- Demo 去重时修复失效本地链接并保留资源 ID/标签。
- 图片预览限制输入 8MB、解码尺寸 4096、分配 96MB；生成 320px 缩略图，缓存约 256 项。网格生成本地缩略图，远端图片仅经主动点击请求，不附加语雀会话凭据、不跟随重定向。
- 本地 Demo 隔离数据目录通过 YUQUE_DATA_DIR 指定，用于真机冒烟测试。

## 验收与边界
- macOS arm64 锁定依赖编译检查、Rust 测试及 Debug 构建；最终 14 项回归测试全部通过；本轮中间构建已实际启动 1160px 宽的 GPUI 窗口。
- 窗口截图未取得，拖放/剪贴板/密钥库的真实平台交互及双主题视觉尚未验收。
- Rustup/Cargo 可用；Cargo 全局缓存存在非致命权限警告，未修改系统目录权限。
- 真实语雀非公开接口尚未使用账号验证；不保证 CDN 可访问性或远端幂等。恢复网络结果不确定的任务前需核实语雀网页端。
- 暂停/取消仅在文件间生效；进度按文件计数，未实现字节级上传限速、进度或断点续传。历史失败列表的完整任务工作区仍待实现。
- 不自动探测会话有效性；账号选择不持久化为默认登录，启动仍遵循环境变量 / Demo 模式。Windows Credential Manager 行为需真机验收。
- 未完成完整跨平台 CI、正式发布、安装包签名或公证。当前版本仍为 0.3.1。
- 主要控制器仍集中在 main.rs，下一轮应优先按账号、资源、队列及视图拆分，并增加 GPUI 交互测试。

## 第二轮集中开发（基线 f6016df，2026-11-24）

### 实施
- main.rs 从 1042 行降至 41 行；desk/mod.rs 管理工作台状态与初始化，account.rs / queue.rs / resources.rs / view.rs 分离业务方法和渲染，仍共享 UploadDesk 状态。
- 独立任务中心按当前账号分页（50 条）、显示 SQLite 持久化错误与状态；单任务重试受账号和状态约束。重启后 running 表示中断／结果未知，不自动发送请求；重试前核实远端状态。
- 语雀上传 multipart 采用 MeteredReader，实际统计 HTTP 库读取的文件字节；每 120ms 刷新，非服务端接收确认，非 Demo 上传的模拟进度。
- 主动触发只读 GET /api/mine 会话检查；参考公开的第三方 Web API 实践（https://glama.ai/mcp/servers/k187401/yuque-mcp），非官方稳定契约。限制响应体大小、不重定向、不记录响应内容；必须匹配配置的账号 ID，成功也不代表上传权限。
- GPUI 原生图标、紧凑工具栏、右栏可折叠；1000px 以下使用窄窗口策略，720×600 最小窗口。保留系统原生标题栏。
- 增加全部/图片/视频/附件分类；名称与标签搜索 100ms 防抖；当前页批量 URL/Markdown、取消选择。
- 缩略图与大图共享缓存及互斥保护；大图最长边 1600px，不放大较小图片；仅清理匹配派生缓存命名的 PNG，不删原文件或任务。远端预览仍需主动触发。

### 本轮验收
- python3 scripts/static_check.py、git diff --check、cargo check --locked --all-targets、cargo test --locked（16 passed / 0 failed）、cargo build --locked 均通过（macOS arm64）。
- 真实 macOS 系统凭据库诊断：YUQUE_KEYCHAIN_SMOKE=1 + 隔离 YUQUE_DATA_DIR + Demo 模式，唯一临时假凭据写入、读取、删除、确认不存在全部成功；未使用真实账号。
- 最终 Debug 程序以隔离 Demo 数据目录启动，CGWindowList 检测到真实窗口 ID 43992；screencapture -l 返回 could not create image from window，未取得截图。未完成真实双主题、缩放、拖放、任务页交互的视觉验收。
- 本轮发现并修复 GPUI IconName 导入错误与小图预览被放大问题；回归测试通过。
- 实际语雀上传、真实 Cookie 会话探测、CSRF 校验及接口兼容性仍待用户授权测试；未自动发出真实账号请求。Windows / Intel CI 未执行。
- 剩余欠账：账号/任务/资源仍共享状态；未做完整 GPUI 交互测试，未实现上传字节级暂停/取消、服务器端确认进度、断点续传、严格幂等或独立标签树；缩略图调度仍按页串行，未完成长时间内存和缓存并发压力验收。
- Cargo 全局缓存权限警告依旧非致命，未改变系统权限。

## 第三轮及离线相册重构（2026-11-24）
- 上一轮未提交的工作区变更纳入本轮：AccountState、QueueState、ResourceState，原生 GPUI 指针测试，标签路径及事务批量标签，预览缩放/平移，任务恢复状态机、历史 unknown 迁移、限速及协作式取消、HTTP 回环异常测试。仍为单 GPUI Entity，未做到完全独立控制器。
- 基于 PicGo 的相册、PicList 的批量管理、PicOne/uPic 的本地缩略图实践，重构默认图片平铺、按需详情、单卡复制 URL、列表切换、紧凑工具栏。未引入外部 UI 源码。
- 语雀图片不再由预览模块发起 HTTP 请求。上传或复用成功后从本地源生成 160px JPEG（质量 45，透明背景合成）；album-previews 永久保存，按账号和内容哈希隔离。资源浏览、放大微缩图、复制链接均离线。
- 历史缩略图：优先现存 JPEG，其次旧版 PNG（含 large-），最后 Demo 文件或账号范围内最多四个已完成同名任务的本地文件；本地源须匹配 SHA-256。没有来源则占位，不自动请求语雀。旧缓存不主动删除，移除原清理按钮以防丢失唯一预览。
- 预览输入限制 32MiB、最大维度 8192、解码分配预算 96MiB；不支持/超限的文件仍可正常记录链接。缩略图生成失败不改变已成功上传状态。不额外复制普通上传原文件；既有剪贴板源文件仍按原机制保存，未删除用户数据。
- 本地验证：常规 20 项测试、含 GPUI 的 21 项 QA 测试通过；测试包括真实 Demo 上传生成缩略图、删除原文件重启后复用、历史任务本地恢复、跨账号隔离、HTTP 回环错误处理、限速/取消。无真实凭据或真实上传。
- CI 增加 macOS arm64 QA 测试步骤，未执行远端 CI。Cargo 缓存权限警告非致命。仍处于 0.3.1 开发版本，不宣称正式发布。
- 视觉脚本使用合成 PNG 测试素材，删除测试原图后再启动应用，输出 dist/qa/album-offline-dark.png；不捕获用户桌面。首次测试素材重复触发正常去重约束，已修正。
- 最终视觉验收：QA 捕获前重复渲染并等待异步图像解码，脚本成功生成 7 张 PNG；已实际审查深/浅相册和窄窗、删除 11 个合成原图后重启的离线相册。1160px 窗口为 5 列，780px 窗口为 3 列。图片渲染正常；窄窗仍需滚动查看后续卡片，未做完整交互动效/长时压力验证。截图为 GPUI 渲染帧，不含系统标题栏。

## 相册深化（2026-11-24，基线 bbdb739）
- 新增 src/desk/album.rs 布局计划：等宽、保留比例的等高拼图（竖图保底 96px）、列表、UTC 日期分组与 96–240px 密度调节；没有独立的严格瀑布流列布局。
- GPUI ListState 按行虚拟化、150 项 keyset 分页；原生滚轮测试确认 150 条资源可滚动，滚动后密度控制生效。布局算法和 SQLite 10,000 条记录完整分页测试通过；尚未测万张真实图持续滚动帧率、内存或 GPU 占用。
- 收藏独立增量表；缩略图 dHash 存在 asset_visual，按需写入，检索同账号现存本地微缩图，汉明距离 <=10，最多 150 项。dHash 只做近似结构比较，存在误匹配。
- 全账号相册备份：VACUUM INTO、quick_check、复制命名合法的派生预览、失败清理新目录。备份恢复测试验证收藏/相似索引；无 UI 覆盖式恢复，迁移通过新 YUQUE_DATA_DIR。
- 离线相册仍只读本地，上传/原文件及账号凭据保存机制不改变。截图测试使用合成不同宽高比 PNG；删除测试原图后重启仍显示。
- 真实渲染 10 张截图；已审查深浅、离线、等高、窄窗密度和收藏。等高首轮发现窄竖图控制越界，修复最小宽度后复验；最后控件仅新增测试选择器。QA 截图不含原生标题栏。
- 首次新交互测试固定坐标未命中尺寸按钮，改用 GPUI debug_bounds 后通过。未验证真实账号上传、跨平台 CI、正式发布；Cargo 全局缓存非致命权限警告不变。

## 沉浸式 UI、瀑布流和桌面集成（基线 305ea5a，2026-11-24）
- 顶部上传、标题、搜索整合，状态条缩小；图片卡片移除常态装饰边框。独立最短列瀑布流按本地微缩图比例排列，按日期分区、可视区虚拟化、原生滚动；资源 ID 锚点保持，极端比例受布局上下限约束。保留等宽、等高和列表。
- 缩略图批量更新（12 项）；已索引资源只读尺寸头，避免重复解码计算 dHash。仍为 150 项游标分页，非一次装载全部万级资源。
- 新依赖 tray-icon 0.26.1、global-hotkey 0.10.0，MIT/Apache-2.0，Cargo.lock 已同步；GPUI 原生通知。macOS ARM64 隔离 Demo 进程真机启动，实际日志 DESKTOP tray=true hotkey=true；仅注册，未触发真实快捷键、托盘上传、系统通知或窗口关闭恢复。Windows/macOS Intel 未执行。
- 完成两次 25 项 QA 测试（0 failures），新增万级瀑布流布局无重叠测试和原生滚动/尺寸调整锚点断言。13 张实际 GPUI 渲染截图，审查深浅及窄屏瀑布流、紧凑网格；不含原生标题栏。
- 运行万级真实编码合成 JPEG（非自然照片）性能测试，CPU draw/dispatch 120 样本，ps 采样 CPU/RSS，详见 docs/performance/album-2026-11-24*.json。未测物理显示 FPS、应用独占 GPU 利用率或长期运行；并发系统负载、Debug 构建及磁盘缓存影响数据，不能据此宣称稳定 60 FPS。
- 保留真实数据，不使用真实账号上传，不执行远端推送或正式发布。Cargo 全局缓存权限警告仍非致命。

## 连续相册与 Release 性能专项（基线 66e481f，2026-11-24）
- 150 项游标批次连续追加，接近末尾自动读取，支持手动补载；保留已选 ID，筛选/账号切换清空。批量选择上限 150，避免破坏标签事务限制。新测试发现 GPUI 滚动回调返回旧范围导致自动加载失败，延迟读取逻辑滚动锚点后修复，310 条记录滚动追加与无重复回归通过。
- 已有 JPEG 使用尺寸头和本地路径预读取；缺失预览仅调度可视区附近，每批 12 项。GPUI ImageCacheItem 原生解码缓存 LRU 192 项，淘汰释放渲染图像，不删除永久微缩图。行布局签名缓存、前缀 splice、瀑布流二分裁剪与资源 ID 锚点减少重算；已加载的元数据仍随滚动累积，极大窗口和长期连续滚动仍待压力验收。
- SQLite desk_preferences 增量表，主题/布局/尺寸/排序/收藏/日期/正常窗口尺寸/快捷键持久化，180ms 防抖、正常退出补存，QA 不覆盖偏好。快捷键 U/I/off 重启生效。
- 剪贴板使用当前 Storage 数据目录，原子写入源文件；隔离 TestAppContext 私有剪贴板真实 Demo 队列生成微缩图通过，无系统剪贴板改写或真实账号上传。
- macOS ARM64 Release QA 构建成功（18,194,752 bytes），10,000 个真实编码合成 JPEG；120 次 CPU dispatch/draw 样本。前台网格 P50/P95 2.057/3.262ms，瀑布流 2.577/4.471ms。原生帧回调 P50/P95 分别 16.667/16.686ms、16.666/16.680ms（各119样本）；已加载 750/600 项，解码缓存均为192；RSS 峰值145.62/143.06 MiB，ps 平均 CPU17.59%/19.95%。整机 ioreg GPU 范围5–14%/5–15%，不可归因于本应用。报告 docs/performance/album-release-2026-11-24.json。初次窗口未前台，原生帧回调为0且超时；增加超时及唤起窗口后复测成功，保留 occluded 补充报告。回调节奏不等于硬件呈现 FPS，缺少 Instruments xctrace，未做单进程 GPU 分析、真实自然图长时压力测试。与上一轮 Debug 数据不能直接当作同规格优化比例。
- macOS 原生托盘/快捷键注册实际成功；并行第二个隔离实例同键注册失败，备用 I 成功；程序化原生窗口 visibility Visible -> Hidden -> Visible。AXIsProcessTrusted=false，未模拟物理菜单点击/快捷键，未修改系统权限。系统通知实际送达未验收。详见 docs/performance/desktop-2026-11-24.json。
- 常规编译、28 项 QA 回归通过；不使用真实语雀凭据，不推送，不发布。Windows / macOS Intel 及正式安装包仍未验收，Cargo 全局缓存非致命权限警告仍存在。
- 最终 Release QA 视觉脚本通过，实际生成14张渲染 PNG（含 persisted-preferences.png）；审查深色瀑布流、窄窗网格、从数据库恢复的浅色/196px/最早优先/收藏筛选/备用 I 快捷键。窗口恢复宽高比断言通过。截图不含系统标题栏；已加载计数分母仍为账户资源总数，不是过滤后精确计数。

## v0.4.0 发布候选（基线 d48dddc，2026-11-24）
- 相册精修：筛选图标、批量操作栏按需出现、卡片选择/收藏/复制统一 GPUI 图标、悬停操作背景；沉浸图片墙释放左右栏，显式退出，独立系统全屏按钮。加载数与账号总数分别标注。原生点击回归验证进入/退出沉浸模式，28 项 QA 通过。
- 已统一 Cargo 版本 0.4.0、锁文件及 CHANGELOG；重构三平台 CI 打包，macOS ARM64/Intel ad-hoc 签名 app ZIP，Windows x64 便携 exe ZIP；每个归档附 SHA256，发布前按候选 SHA 核验。
- 真实 GPUI 截图目录 dist/qa，重点 album-immersive.png 和 resources-dark.png；未验证系统全屏物理点击、托盘点击、全局键实际触发、通知送达。当前 AXIsProcessTrusted=false，不修改权限。不使用真实凭据。
- 本地候选与远端 CI 进度需以 git/gh 实际状态为准；正式 Release 需三个架构全部通过并校验资产后执行。暂无开发者签名/公证证书，不声称 macOS 可无 Gatekeeper 提示安装。
- 候选 2eed0fd 的 CI 45430828149 Windows job 138575344377 在 Static checks 失败：原始日志确认 Python 默认 charmap 解码中文源文件报 UnicodeDecodeError；静态检查/版本读取改为显式 UTF-8。需检查新提交的全部三平台结果，不使用旧候选产物。
- 指定候选 5c96a9a 的 CI 45431125297，Windows job 138576380836 静态检查通过，24 项测试中 23 通过、1 失败；原始日志证实文件 URL 往返去除 Windows 扩展长度路径前缀，测试直接比较普通路径与 canonicalize 路径失败。仅修复测试路径规范化，追加真实文件读取及 SHA-256 断言，保留原业务行为。新候选仍须三平台完整 CI。冻结新增功能。

## v0.4.0 正式发布（2026-11-25）
- 用户明确将发布范围缩减为 Windows x64 与 macOS ARM64。CI 45432243907 两目标完整成功，Windows 路径兼容性修复已通过。Intel 不作为发布门槛。
- 正式 Tag v0.4.0 -> 6f485d8b01386774fa395662838e23f756430f35，Release https://github.com/weepwood/Yuque-Asset-Desk/releases/tag/v0.4.0，非草稿、非预发行。
- CI 原始 ZIP 下载后独立验证 SHA256、CRC、路径安全、架构；macOS Info.plist 版本及 ad-hoc 签名通过。实际启动 ARM64 CI 包，窗口 1160x828，隔离数据库、托盘及快捷键注册成功；有 sandbox_extension 非致命权限警告。Windows 不进行真机启动，系统通知/实际快捷键/真实上传未验收。
- ARM64 ZIP 7854534 bytes，SHA256 75d012724d6b53b639043bb597e4cd28f4e743b9967c0c4e039a3e9cb6219077；Windows ZIP 8917151 bytes，SHA256 93eae86d299cf82ee7c6af35fe09fc13afa355e34fe6c1c0ae57110b43791a90。
- 发布后核对全部四项资产的名称、状态、大小与 GitHub SHA256 digest，全部一致。详细本地审计目录 dist/release-audit/45432243907-two-platforms。保持发布 Tag 指向构建源码，后续文档提交不改变发布二进制。

## V0.5.0 UI 重构（基线 e2dbff8，2026-11-25）
- 完成 GPUI Kit 0.9.0 本地源码 API 审计及官方设计指南/Eagle 工作台参考；复用 SidebarMenuItem、DropdownMenu、PopupMenuItem、ContextMenuExt、Progress，未添加 UI 依赖。
- design.rs 集中语义颜色、边界尺寸、圆角、状态动效参数；workspace.rs 组织导航、筛选/布局菜单、设置及卡片菜单。相册主工具栏一行（窄窗搜索另起一行），标签左对齐，设置内容最大宽度 680px，任务列表弱分隔线；图片常驻操作移至悬停/选中/右键，卡片名栏 44 -> 24px，保留文件名、收藏标记及全部资源默认入口。
- 保留数据库、上传、凭据、备份、永久离线微缩图和 192 项解码缓存。未新增业务功能或自动请求远程图片。版本号与锁文件更新 0.5.0，未推送/发布。
- 本地 macOS ARM64：静态检查、cargo check --locked --all-targets、常规 24/24、QA 29/29、QA build、19 张真实渲染 PNG 全部通过。新增原生菜单点击/键盘排序、布局、悬停、右键收藏回归；既有滚动、锚点、上传隔离、设置、离线缩略图测试仍通过。视觉截图使用 Pillow 合成不同类型图片，删除测试原图后仍显示预览；不是自然照片或真实用户数据。
- 已实际审查深浅主界面、窄窗、瀑布流、悬停、设置、标签及任务列表。窄设置需滚动；菜单原生交互测试通过但未截图覆盖所有菜单状态。未逐像素复刻商业产品，也未进行完整辅助功能、Windows 真机或 Release 性能/跨平台 CI 验收。Cargo 全局缓存权限警告非致命。
- 截图目录 dist/qa；设计规范 docs/ui-design.md；本轮不改 v0.4.0 正式 Tag 和资产。

## V0.5.0 高级视觉精修（基线 ac7739e）
- 统一标题/文件名、图片间距、图标、主按钮与浮层配色；新增原生密度 Slider 与 Switch、快捷键菜单、Empty 与复制反馈 Notification，保留原有功能；详情栏预览优先、悬停渐变及无布局变化的选中轮廓；详情淡入与库原生动效。
- 实际 macOS QA 30/30，新增预览适应几何测试及原生密度滑块点击/持久化同步断言；前一轮本地常规测试 24/24 和 cargo check 通过，最终 QA build 通过。完整 24 张真实截图生成后发现窄窗详情裁切、100% 预览裁切；修复后对 8 张图重新捕获并检查窄窗关闭按钮、完整图像比例、详情和应用内通知。
- 截图为隔离 Demo 数据，通知使用演示消息；未写入真实系统剪贴板，未请求语雀原图，未执行真实账号上传。没有新增业务功能；未实测 Release 性能、Windows 真机、完整无障碍和所有动画时序；本轮不推送、不发布。后续仅 Windows x64/macOS ARM64 发布。

## V0.5.0 上传接口与发布质量收口（基线 66d08a4）
- 上传协议、数据库、离线预览和现有账号模式保持不变。将队列暂停从错误字符串匹配改为结构化 HTTP/远端结果分类，401/403 更新设置中的会话提示，429 和 unknown 仍暂停；状态保存失败仍按待核实处理。JSON 响应解析独立，新增成功/拒绝/非 HTTPS/畸形 JSON 契约测试，回环 HTTP 测试验证暂停分类。
- CI 矩阵删除 Intel Mac 构建，仅保留 Windows x64 和 macOS ARM64；不改变现有测试与打包门槛。未使用真实语雀凭据或请求远端原图。
- 本地 macOS ARM64 QA 31/31 通过，静态检查与 diff check 通过；Release 性能、视觉及远端验收需以下续实际报告为准。
- 最终本地常规测试 26/26、完整 QA 31/31、cargo check --locked --all-targets、Release QA build、24 张实际渲染截图通过。审查深色主界面、窄窗网格、瀑布流、窄窗详情、放大预览、浅色设置；保持既有视觉结构，无新增 UI 功能。窄窗详情仍需向下滚动。
- 10,000 张合成 JPEG Release QA 性能复测见 docs/performance/album-v0.5.0-release-2026-11-25.json。网格/瀑布流 CPU draw P50 1.514/1.300ms，P95 2.037/1.558ms，峰值采样 RSS 142.86/140.83MiB，平均 ps CPU 26.38%/14.61%，原生回调 P50 16.665/16.666ms；加载 750/600 项，解码缓存均为192。网格 CPU 采样较上轮增加，系统负载与采样条件未完全控制，不宣称全面优化。原生回调非真实呈现 FPS，整机 GPU 样本不可归因单进程。未做长期滚动、Windows 真机、真实语雀上传或通知送达测试。
- 正式发布仍需新候选双平台 CI 和下载产物验收；不移动 v0.4.0 Tag，不提前创建 v0.5.0 Tag。

## V0.5.0 舒适模式及上传交互（基线 7dacbe3）
- 舒适模式默认开启，紧凑模式保留旧相册间距；持久化兼容旧偏好。全局 spacing 同步相册布局、导航、表单及详情；切换后刷新测量和滚动锚点。图片尺寸、选择与筛选独立。
- 相册全内容区支持原生外部文件拖入、悬停反馈；忙碌拒绝有提示。活动批次显示处理进度和请求体字节读取量；待恢复任务底栏入口；鉴权/限流/unknown 暂停原因保留到批次结束。没有自动重传。
- 本地常规 26/26、QA 32/32，实际模拟 GPUI FileDropEvent 拖入隔离图片，生成资源和永久微缩图；验证忙碌拒绝、Switch 持久化、两种间距的万级布局及瀑布流锚点。首轮 27 张真实 Debug 渲染截图，最终局部复验 12 张；实际审查舒适/紧凑宽窗、窄窗、详情、浅色设置，宽窗 148px 相同目标密度下 5/6 列。没有实测最新 Release 性能、Windows 真机或真实语雀上传。
- 指定旧 CI 45443300982 Windows 成功，macOS 排队；新增改动必须新候选重新完整验收，不使用旧产物正式发布。

## V0.5.0 检查更新（基线 da4fe0d）
- 设置首部新增关于与更新，当前 Cargo 版本、手动检查、检查中、无更新、发现新版本、网络失败及历史版本入口。独立 reqwest 客户端仅读取固定公开 GitHub Release API，不发送语雀凭据。SemVer、防降级、拒绝草稿及预发行，按两种正式平台匹配 ZIP/SHA256；缺资产时仅提供 Release 页面。只打开下载链接，不自动安装或校验已下载的文件。
- 本地常规 28/28、QA 34/34（各含一个默认忽略的公网测试）；显式实际公网测试 1/1 成功，返回 Current("0.4.0")，候选 0.5.0 不提示降级。深浅主题更新可用状态合成 0.6.0 截图及其他基础图共6张成功，已审查两张更新截图。未发布 0.6.0、未执行 Windows 真机或真实新版本下载安装测试。
- 需以最新提交重新执行双平台 CI 后发布 V0.5.0，不能混用旧产物。

## V0.5.0 品牌资源与 macOS DMG（基线 ebcd2d5）
- 新增原创 1024px PNG、ICO、ICNS、macOS 单色托盘模板、Windows 彩色托盘和 640×420 Finder 背景；均在 assets/branding，可使用 scripts/generate_branding.py 再生成。Windows build.rs 嵌入 ICO/产品版本；CI 验证 PE 图标/版本资源类型及实际产品名版本。macOS Bundle 写入 ICNS 并声明 CFBundleIconFile，保留 ad-hoc 签名。
- 采用 dmgbuild 1.6.7、ds_store 1.3.4、mac_alias 2.2.4，固定安装布局（165,205 / 475,205），Applications 指向 /Applications。正式交付改为 Windows x64 ZIP、macOS ARM64 DMG 及各自 SHA256；更新检查优先 DMG、旧 ZIP 回退；CI 加入 assets/build.rs 路径触发。修复 verify-only 重写校验摘要问题；故意错误摘要测试确认拒绝且不重写。
- 本地 QA 35 通过、1 公网测试默认忽略；正式非 QA Release 构建通过。初次 DMG 验证脚本错误假设背景在子目录，实际为卷根 .background.png，已修复并复验 DS_Store、背景实际引用、卷图标、App 图标/版本/架构/签名及 Applications 链接。最终 DMG 8716435 bytes SHA256 d940e67bbcec6d14fe2c5c951ad846731c5e750f7dae7deba87e2e582ac2e0df。独立 verify-only 成功；本地 dist/branding-release。scripts/smoke_dmg.py 从只读 DMG 复制至隔离目录、签名、窗口 1160×832、数据库、托盘/快捷键注册通过；有已知 sandbox_extension 非致命警告。首次 smoke 未显式启用隔离桌面集成，修正测试环境后成功。没有安装至用户 Applications，没有真实账号上传。
- macOS 屏幕录制权限未授权，未捕获真实 Finder 窗口；仅审查实际背景 PNG、主图标与挂载后布局元数据，未宣称物理拖拽安装或 Gatekeeper 公证通过。Python pip 默认信任链错误，使用系统 /etc/ssl/cert.pem 保持 TLS 验证完成安装。旧候选 ebcd2d5 双平台 CI 45445083095 均通过，新候选仍需完整双平台 CI，未发布。

## V0.5.0 存储管理与卸载辅助（基线 ecfc9f7）
- 设置中的独立存储页提供实际逻辑文件大小、打开目录、后台统计、轻量清理、隐私清理、两阶段重置与卸载指引。复用 GPUI Kit Button、主题和 GPUI 原生确认提示。轻量清理只删除 clipboard 中哈希可核实且跨账号无任何任务/Demo 文件链接引用的普通文件；保留同名引用、未知文件、永久离线微缩图和外部文件。
- 新账号先保存索引再写系统凭据，避免索引写入失败导致新的孤立凭据。已确认不存在才移除索引，失败保留；当前账号凭据删除也须确认。无法枚举旧版孤立凭据，未处理环境变量及浏览器凭据。
- 重置须先处理所有账号未完成/失败/unknown 任务，提供备份入口、警告及二次确认。凭据处理失败不启动重置；助手等待实例锁、验证一次性标记、重检任务与索引、只删除明确管理文件。不递归删除根目录；未知文件、实例锁及错误报告保留；文件删除非事务性，失败可部分完成。卸载步骤见 docs/uninstall.md。
- 本地 macOS ARM64 常规测试 33 passed/1 ignored、QA 39 passed/1 ignored，cargo check --locked --all-targets、静态检查、QA build 通过。真实 GPUI 交互测试点击存储入口、统计、第一阶段重置并确认数据库仍存在；测试部分凭据清理只用模拟索引，不读取真实系统凭据。scripts/data_reset_qa.py 独立进程隔离验证错误 token、unknown 阻止删除、等待实例锁、释放后删除及外部原图/备份/未知文件保留。7 张实际渲染截图（3 张新存储页与基础/设置图），深浅/窄窗审查通过，窄窗需滚动。未执行真实系统凭据删除、原生确认框完整端到端、Windows 真机卸载、最新正式包启动或实际磁盘占用统计。未修改用户实际数据目录。非致命 Cargo 全局缓存清理权限警告仍存在。

## V0.5.0 正式发布收口（2026-11-25）
- 候选 bdb29ac，CI 45447816768 双平台完成 success，无实际失败项需要修改代码。静态检查、本地 QA 39 passed/1 ignored，独立重置进程复验成功；全程隔离数据目录。
- 两个平台直接下载指定 CI 原始产物并 verify-only（不重写原始 SHA256）。macOS DMG 8855946 bytes，SHA256 cfa0ffd0b2bf0049ac161907adbdac46030346a4446f2e5db0b6e029e0d5f3ef；Windows ZIP 9245630 bytes，SHA256 fad393c7850ed32149c792679abccff686f3fb89ebb8ee7a72e82a2544d6d107。DMG 结构、背景、图标位置、签名、架构、版本与 Windows PE 图标/版本资源和 ZIP CRC 均通过。
- 实际 CI DMG 应用复制到临时目录并启动（非 QA 正式构建），窗口 1160×828，隔离数据库初始化，托盘/快捷键注册成功；有非致命 sandbox_extension 日志、hdiutil 弃用提示。未安装至 Applications，未触碰真实凭据或用户数据。未验证 Windows 真机、真实批量凭据删除、Apple 公证、真实上传或系统通知送达。
- v0.5.0 正式 Release 已公开（2026-11-24T17:27:44Z），Tag 指向 bdb29acc6983b248a39d7481a3904c82789d74c8。草稿先核对四项资产的 GitHub SHA256 digest 后公开；公开后重新下载四项资产，全部大小、digest 及包内外校验一致。公网更新检查返回 Current("0.5.0")。本地验收产物位于 dist/release-audit-v0.5.0/，未提交二进制。
- 发布说明 https://github.com/weepwood/Yuque-Asset-Desk/releases/tag/v0.5.0 。后续文档提交不改变已发布 Tag。

## V0.5.1 账号自动识别（基线 7482255）
- 核对语雀官方 yuque-open-cli README 和内置 OpenAPI 2.0.1 YAML（官方文档网页无法直接读取，使用官方仓库规格）。GET /api/v2/user，X-Auth-Token，data.id/name/login/type；拒绝 Group/未知类型。Token 仅用于识别，不持久化，不请求头像。Cookie-only 使用既有 /api/mine，保存时重新获取身份并核对，CSRF 自动提取及冲突检查，禁止错误 Token 静默降级。旧版系统凭据、任务及相册数据兼容。
- 账号独立设置页、遮蔽输入框、识别反馈、官方 Token 获取入口；移除手填 ID。成功后保存原有 Cookie/CSRF 格式并清空输入。昵称/登录名为本次识别展示，不新增元数据持久化。验证失败保留输入用于修正，不写入账号索引；密钥库失败保留索引。
- 本地常规 36 passed/1 ignored、QA 42 passed/1 ignored；真实回环请求验证认证头隔离、302/401/429，合成响应验证身份匹配/团队/缺失 ID/CSRF 冲突/Token 不序列化，原生空表单交互测试确认不入库。深浅与窄窗三张账号页实际渲染截图审查，浅色已识别状态使用虚构账号；窄窗低频管理需滚动。未用真实 Token/Cookie 调用语雀，未测试真实上传与 Windows 真机。V0.5.1 待双平台 CI，未发布。

## V0.5.1 macOS 文件选择闪退修复（基线 fab3873）
- 用户报告点击上传即闪退；本机未找到匹配的 macOS DiagnosticReports 或统一日志异常，未确认原崩溃堆栈。旧实现 rfd 同步 NSOpenPanel/runModal 与 GPUI 事件循环存在冲突风险，GPUI 原生实现已具备异步 completion 与 nil 防护。上传、备份两处改用 GPUI prompt_for_paths，移除 rfd/独占传递依赖（离线锁文件更新），新增 picker_pending 并处理取消、失败、重复触发、账号/清理/退出互斥。
- 本地 cargo check --offline --all-targets、静态检查、QA 43 passed/1 ignored、QA build 通过。新增真实 GPUI 模拟点击和原生平台选择回调，验证取消后再选择、Demo 图片入库与永久微缩图、备份取消及成功、重复选择和期间入队拒绝。未使用真实语雀凭据。
- scripts/picker_native_qa.py 在实际 macOS 进程中以隔离 Demo 打开上传和备份系统窗口；两者均观察到独立 880×448 原生面板及 1160×832 主窗口，9 秒后进程存活，GPUI 回调仍执行，未新增任务。第一次 5.5 秒观察未及时收到回调，延长后通过。测试进程由脚本终止，未实际用鼠标操作 Finder 或 Windows 真机。未替换已安装 v0.5.0；新候选待双平台 CI 与正式发布。

## V0.5.1 舒适模式二次优化（基线 1c25eac）
- 舒适模式主内容边距 24→32，图片间距 16→24，日期 36→44，区块 16→22，列表内边距 16→20，导航间距 6→10，宽侧栏 184→200，详情 280→320，图片名区域 24→32；字号 21/14/12。紧凑参数不变。工具栏标题独立、Medium 按钮和换行，底栏归组；窄窗品牌截断在首轮截图发现后修复，详情名称/属性、账号标签/输入归组。
- macOS QA 44 passed/1 ignored，QA build、静态检查、diff check 通过；首轮 15 张实际渲染图，最终局部 8 张（含基础/持久化图）。审查舒适/紧凑深色、浅色、窄窗、详情和账号设置。宽窗相同 148px 目标尺寸舒适 5 列/紧凑 6 列。新增 720×600 双密度上传按钮边界、点击与取消、图片尺寸不变回归；原生文件选择模拟回归及万级布局测试仍通过。窄窗低频设置和详情需要滚动；没有重新执行真实系统文件面板或 Release 性能基准、Windows 真机验证，未使用真实凭据。V0.5.1 未正式发布。

## V0.5.1 原生 UI 与动效精修（基线 b31de8b）
- 调研 PicView 虚拟图库、Zed dock 和 GPUI Kit motion，按现有 gpui-base 0.9.0 API 实施，无新增依赖/素材。舒适模式尺寸按钮组合、筛选/沉浸选中状态、日期标题统一、详情收藏/相似资源操作组。修复任务页旧详情仍可能出现、详情可见性与主区宽度不一致的问题。
- 原生 keyed transition：可见卡片 hover 120ms、详情淡入180ms、预览缩放160ms（独立 session）；页面进入采用180ms短淡入，不阻塞布局。初始状态不播放，退出隐藏按钮立即不可点击、详情立即移除，未实现几何退出动画。尊重减少动态效果设置，不引入循环计时器。
- 本地 QA 45 passed/1 ignored、QA build、静态检查和 diff check 通过。原生渲染测试用可控时钟验证实际中间插值、快速反向连续性、结束不重播及 reduce_motion。真实渲染13张（深浅/窄窗/紧凑/瀑布流/悬停/详情/预览/设置/任务等），审查主墙/窄窗/瀑布流/双主题详情/预览。使用合成图片与隔离数据库，无真实凭据或用户数据修改。未录制真实动效视频，未跑新的正式性能基准、Windows 真机或双平台最终发布验收。V0.5.1 仍为候选。

## V0.5.1 面板、浏览与设置集中精修（基线 b80d9db）
- 基线 CI 45459645802 双平台均成功。参考 Zed 面板、PicView 浏览和 GPUI Kit 原生 motion，保留 GPUI Kit 0.9.0，不增加依赖，不修改上传、凭据、数据库、缓存架构或发布平台。
- 共享圆角 8px，设置细边框分组、资源/账号图标入口，收起侧栏为 56px 图标栏；窄侧栏品牌缩写为“语雀”（实际截图发现英文截断及副标题换行后修复）。面板固定宽度短滑入、沉浸切换淡入，关闭立即移除，不逐帧重排图库。保留原生菜单/确认框/Tooltip/通知，未彻底改造浮层行为。
- 图片双击进入预览，首次单击即选择，当布局变化延迟 500ms 开详情；预览独立焦点，左右键切换已加载图片，+/-/0/Esc，滚轮指针锚点、边界拖动。滚轮直接响应，按钮缩放保持160ms插值；不自动加载下一页或远程原图。侧栏折叠仅会话内状态。
- 首轮完整 QA 中设置页切换发生栈溢出，拆出独立 preview_view 后默认栈复验通过；预览渲染提前返回，跳过图库布局。最终功能完整 QA 47 passed/0 failed/1 ignored，cargo check --locked --all-targets、QA build、静态检查及 diff check 通过；最终品牌文字修正后专项交互测试1/1与构建通过。新增合成几何测试和实际 GPUI 窄窗双击、键盘切图、滚轮缩放、拖动释放、侧栏点击证据。既有异步选择、降低动态效果、连续滚动回归通过。
- 首轮13张真实渲染截图、复验6张、最后4张（含重复基础图）；审查双主题收起侧栏、宽窄图库、详情、设置及预览，最终窄栏无文字截断。截图均隔离 Demo 合成数据，位于 dist/qa。未录制实际动效视频，未测试物理鼠标/系统双击间隔变化/Windows 真机或重新跑 Release 性能基准。新候选未正式发布。

## V0.5.1 发布候选复验（2026-11-25）
固定候选 efe4ef9eb8e0f44b0f1914063257ddd2555c782f；CI 45470121925。完整 QA 47 passed/1 ignored、QA build 和静态检查通过。原生上传/备份选择面板实测正常且 GPUI 回调执行，隔离任务为0；独立重置安全测试通过。Windows CI 成功，同一 artifact 首次下载 EOF 后重试，ZIP 9283648 bytes，SHA256 c5b592e962ae7c0ebf4f7aa92e14817e849379962a7d11aec8cffe5860af7e42，verify-only 成功。macOS runner 仍 queued，未取得 DMG，未发布。证据与候选发布说明保存在 dist/release-audit-v0.5.1/（ignored）。本轮只提交交接文档，未修改源码、CHANGELOG 或 Tag。未覆盖 Windows 真机、真实语雀上传/凭据删除、macOS 正式包及 Apple 公证。

- 发布进展：CI 45470121925 attempt 1 macOS 被取消（GitHub check annotation: The job was not acquired by Runner of type hosted even after multiple attempts；macOS arm64 capacity constraints）。仅重跑 macOS job，attempt 2 macOS 138727948374 queued、Windows success；不修改源码、不重复本地测试。正式 Tag/Release 尚未创建。

- V0.5.1 已正式发布：45470121925 attempt 2 success；Tag efe4ef9；2026-11-24T19:55:48Z 公开，四项资产重新下载与原始 SHA256/GitHub digest 一致。Windows ZIP 9283648 bytes，macOS DMG 8887805 bytes。DMG 签名、架构、布局和隔离正式启动通过（1160×828，tray=true，hotkey=false 因已注册冲突），未改真实数据。公网更新 Current("0.5.1")。后续仅文档提交不改变 Tag；证据 dist/release-audit-v0.5.1/。

## V0.6.0 预览专项（基线正式 v0.5.1）
- 独立分支 feat/v0.6-preview：按需本地/在线原图（哈希一致才展示），仅当前一张内存高清 RenderImage；输入32MiB/1600万像素/单边8192、显示最长边2560，默认永久160px离线微缩图不变。网络限制固定语雀CDN HTTPS、无认证头、无重定向，尊重系统代理，不自动请求；其他地址/CDN转换或失败可浏览器打开。无需新增依赖或 schema。预览底部最多七张附近微缩图，可直接切换；向后跨分页追加并跳过非图片，保留筛选范围、可返回已加载页；单次最多九页。详见 README/AGENTS。
- 本地 QA 51 passed/1 ignored、QA build、静态检查、diff check 通过。隔离 GPUI 测试覆盖点击高清/缩略图/释放、跨150条页面边界与回退、缩放/拖动/退出。HTTP回环验证无认证头、200/302/403/超限；源文件哈希/安全限额/颜色通道/下采样验证。真实渲染7张，其中4张预览（深浅窄窗高清及离线）已审查，位于 dist/qa/preview*.png。未用真实用户数据或凭据、未请求真实语雀 CDN，未执行 Windows 真机或新 Release 性能基准；新版待 PR CI，未发布。

## V0.6.0 预览专项收口（基线 ebb7c6d）
- 复查 PR #3 的 CI 45476368867，Windows success、macOS queued。未放宽门禁，继续在同一分支修复实际体验差距。
- 预览缩略图恢复按选中资源的七图范围而非旧图库视口调度；主图本地恢复同时更新路径和比例，尾部向前补齐导航。后台上传触发刷新时保留预览游标/选中项，退出后统一刷新。高清加载失败不覆盖成功的图，支持显式切回离线，加载/错误独立展示；原文件打开失败可见。未新增依赖或持久化缓存，切换来源时可能短暂共存两张高清图，后台阻塞请求仍受20s超时约束。
- 51 passed/0 failed/1 ignored；QA build、all-targets check、static/diff check；增加原生交互断言（失败保留、离线释放、跨页实际缩略图和2:1比例、延迟刷新、源文件保留）。重新生成7张真实渲染截图，其中深浅/窄窗高清与离线4张预览。无真实账号/CDN/Windows真机/正式内存性能测量，未发布新版。

## V0.7 设计重构第一批（候选，非正式发布）
- PR #3 固定 head 5410fc2，双平台 CI 45477696693 成功，合并 main 61a39f6。
- 根 DESIGN.md、固定来源 docs/design-references.md；实际审查 Spacedrive/Immich 参考图。原图和前后 QA 截图存于 ignored dist/design-research/。
- Page 枚举替代互斥页面 bool；shell Command 统一部分命令；独立 InspectorModel Entity 异步标签读取，代际守卫；选择纯模型，单击不打开检查器；Shift/修饰键多选、Space、Escape、方向键、显式详情，统一菜单入口。隐藏页面不再构建图库。视觉改变为稳定工具栏、固定选择命令区、单密度控制、图片文件名/大小、分组属性和连续设置行。
- 真实渲染 QA 14 张；实际 GPUI 注入操作8阶段通过，dist/design-research/path/operation-path.gif（逐帧渲染，不是物理鼠标录屏）；状态证据 docs/performance/v0.7-design-path.log。10000合成 JPEG debug 采样：grid CPU draw p50 9.675/p95 12.938ms RSS154.38MiB；masonry 9.797/11.412ms RSS158.44MiB；各120样本、各300已加载、解码缓存192。原生回调无样本，不等于 FPS；不能直接与历史 release 比较。详见 docs/performance/v0.7-design-debug.json。
- 剩余：继续以真实截图做布局精修，独立 Gallery/Viewer/Queue/Account Entity 与设置页拆离 inspector；写操作仍同步，必须先确保数据清理、退出和账号切换串行安全；统一原生 GPUI Action/key context、真正几何方向导航、焦点/菜单完整路径、宽度过渡与中断验证。当前窄窗与舒适/紧凑已作本地验证，仍需 Windows 真机及 release 性能对照。不得宣称全部六专项完成。
- 后续集中迭代：先检查本批 PR 固定 head CI；仅通过且审查无阻塞才合并；失败仅修复实际错误；每轮一批原子提交，优先上述剩余问题，不增加业务功能，不修改已发布 tag，不绕过分支保护。避免重复开启 PR、并行修改同一路径、覆盖用户本地未提交代码。

## V0.7 交互核心与设计规范第二批（基线 main 5782dd0）
- PR #4 固定 427868b 双平台 CI 45480921070 success，合并 5782dd0；main CI 45532340902 success。审查 PR #1 GPUI 迁移和 #2 原生拖放/批量 Markdown 后关闭重复历史 PR，保留分支与说明（专用本批复制按钮未原样保留，现相册多选复制）。独立 feat/v0.7-interaction-core。
- 独立 ViewerModel Entity 持有高清请求/图片/状态和代际，InspectorModel 错误与重试；收藏/标签/批量写移入 mutations.rs 后台任务，resource_write_busy 阻止冲突操作。原生 GPUI Action、网格/拼图/列表及瀑布流几何方向键、多选预览返回恢复。设置从 inspector 拆离、固定常规/账号/存储；顶部账号入口、选择指引、状态合并、文件类型区分、多选详情。检查器保留固定空间退出，禁用输入，反向可中断，结束单次重排。保持原上传、凭据、原始文件及永久微缩图，无依赖或数据库迁移。
- 55项完整 QA 通过（1公网默认忽略）；实际 SQLite 写锁及 UI dispatch <1s、互锁与持久化，面板实际宽度/反向/减少动态效果，选择与高清生命周期。cargo check --locked --all-targets、静态检查、QA debug/release build 通过。最终15张真实渲染截图，实际检查720×600图库/设置/高清成功及离线回退、浅色图库/账号和详情；8阶段注入鼠标/键盘 compositor 截图/状态均通过。不是物理录屏。证据 dist/design-research/core-after/、path/、docs/performance/v0.7-interaction-path.log。
- Release QA 万级合成JPEG（本地、无网络）grid P50/P95 2.199/5.590ms、RSS148.12MiB、cache192；masonry 4.550/6.332ms、RSS142.16MiB、cache191；各120 CPU draw样本、300 loaded、未超时。非显示FPS；相比历史 V0.5.0 的 P95 更高，版本/系统负载不同，不能归因或宣称提升。记录 docs/performance/v0.7-interaction-release.json。
- 剩余：Gallery、Queue、Account 完整独立 Entity 及统一全部 SQLite 写入（任务索引、凭据索引、偏好仍同步）；完整侧栏/沉浸空间过渡、全键盘与原生浮层路径、真实 Windows 真机/真实语雀、正式包与受控性能对照。仍开发候选（Cargo 0.6.0，V0.7 尚未发布），不声称七项全部完成。未改真实数据/凭据或已发布 Tag。

## V0.7 第三批：队列与持久化生命周期
- PR #5 固定 ee6863b，CI 45533782259 双平台成功，合并 abc943e；main CI 45535347941 成功。
- JobHistory Entity 独立账号范围、代际、任务分页和异步 COUNT。queue_prepare 后台执行 enqueue/retry/resume/ack 与剪贴板落盘，准备至执行保持互锁；任务历史回调替代同步恢复计数。任务中心准确标注“可恢复”。剪贴板只复用 hash 校验通过的已存在文件，硬链接无覆盖发布，非支持文件系统安全报错。
- 偏好快照后台合并保存及 revision 顺序门禁，正常关闭先核验 busy，最终同步 flush 兜底；重置准备禁止启动设置写入。账号索引保存、凭据删除后的索引删除/列表重读均后台执行，保持先索引后 vault、失败保留索引。不新增依赖/数据库迁移/业务功能。
- 59项 QA 通过、1公网默认忽略；all-targets、QA build、static/diff；SQLite 真实写锁下重复入队拦截、最新偏好持久化、账号 scope/分页、剪贴板校验。原生上传和备份选择窗口存活及回调通过（未物理点选），隔离 reset helper 通过（无真实账号）。8张实际截图，任务页/最小窗口/存储已审查。未再次运行全量 Release 性能；沿用上轮基准，不宣称性能提升。
- 剩余：Gallery/Queue runner/Account 完整独立 Entity、启动与重置预检查等同步 I/O、最终退出 flush、其他维护任务、完整侧栏/沉浸动画、Windows 真机与真实语雀、正式发布。保持 v0.5.1 Tag/Cargo 0.6.0。

## V0.7 第四批：运行边界与发布准备
- PR #6 固定 54b3831，CI 45536097492 success，合并 c2bb22d，main CI 45540060853 success。独立分支基于同内容 54b3831。Cargo/lock 升 0.7.0 候选，CHANGELOG 更新，保持 v0.5.1 正式 Tag。
- QueueRunner/AccountModel/GalleryQuery retained Entity 独立任务生命周期，shell 使用 typed event 协调；Gallery 显示/选择/布局仍在 shell，当前 Config 与维护互锁仍由 shell 协调。任务终态一次图库刷新。启动偏好单次读、账号索引异步、重置预检/确认后预检/启动 helper 后台。正常退出确保最新 revision，强制/原生外部退出 best-effort 后台保存（GPUI 200ms预算），数据库启动初始化仍同步。
- 任务固定列、账号紧凑行、底部对齐完整缩略图，侧栏/沉浸保留空间至终态、快速反向、减少动效及焦点恢复。61 QA 成功、1公网默认忽略、all-targets/QA build/static/diff；15张不同真实渲染图（0.7候选账号/设置复验），8阶段操作 trace。macOS upload/backup native panel 存活与回调、隔离 reset helper 成功，无真实账号/真实数据/物理鼠标或 Windows 真机。新增依赖与schema迁移均为零。
- 待完成：候选双平台CI/安装包验收、受控性能对照、Windows真机及真实凭据边界、正式 Tag/Release。不能把架构任务全量迁移或系统强制退出可靠性视为已完全实现。
- 已完成本批 Release A/B 同条件采样：54b3831独立worktree，候选0.7.0；网格P95 5.011→2.260ms（RSS149.55→133.55MiB），瀑布流4.732→1.775ms（RSS147.16→133.39MiB），各120样本、同布局最终加载600/450条一致、缓存192、无超时。原始JSON及比较表见 docs/performance/v0.7-runtime-comparison.md。单次对照不能推断因果、持续FPS或Windows表现。正式双平台CI/产物验收仍待本PR。

## V0.7.0 发布候选收口
PR #7 已通过双平台 CI 45541767866 并合并 b58ee1b（源码树等同固定候选75c173b）。原始双平台包及SHA256验证通过，macOS正式DMG隔离启动成功；Windows真机离线，未完成实际运行与交互验收，保持 v0.5.1 最新正式版本，不创建新Tag/Release。验收详情与四项原始资产摘要见 docs/release/v0.7.0-acceptance.md、v0.7.0-verified-assets.json；复用原始候选产物，不重复全量测试或新增功能。

## 后续设计：浏览上下文独立分支
V0.7合并后CI45542564057双平台成功，固定75c173b原始产物保持冻结，Windows真机门禁未解除。新分支基于5b0c33c，依据DESIGN.md第11节集中实现组合筛选可见、显式范围、准确加载/结果数、相似查询返回原条件、空态恢复、分类高亮及图标栏账号入口。62 QA pass/0 fail/1 ignored，实际720×600指针/键盘回归，8种渲染截图（最终重点复验3张），static/diff与QA build通过；未新增依赖/数据库迁移/网络功能。截图证据dist/design-research/browse-after/；不修改已验收发布候选，暂不合入main或发版。

## 后续设计第二批：查看器、上传反馈与详情
PR #8 初始934ef7a双平台CI45544055365成功；集中追加同一PR，不改变V0.7固定候选75c173b和发布门禁。DESIGN.md第12节落实相对缩放/图片序号/完整缩略图/停止等待、任务请求状态与取消优先级、原生待核实确认及窄窗详情优先操作。64 QA pass/0 fail/1 ignored，all-targets/QA build/static/diff；11种真实渲染图dist/design-research/flow-after/，活跃任务为隔离QA夹具；最小窗口实际检查。无新依赖/schema/网络功能，未做真实语雀上传、Windows真机及全量Release性能复测。

## 后续设计第三批：选择边界与图库错误恢复
PR #8 原 head f0d9f27 双平台 CI 45546108736 success。修复正反向范围超过150项时目标丢失、全选锚点、整页读取失败保留旧详情及预览任务、主项消失时选中集合与详情不一致；追加失败保留已有结果并回退游标，工具栏明确选择上限。原始候选75c173b、正式v0.5.1及发布门禁保持不变。本地 all-targets/static/diff、专项及完整 QA 65 passed/0 failed/1 ignored；错误通过隔离 GPUI 状态事件注入，非真实磁盘故障。未重新采集视觉证据、Windows真机或Release性能，不扩大业务范围。

## 后续设计第四批：可取消图库查询与相似检索
基于d09e6b3继续同一PR。协作式取消与原generation双重保护；未索引图片keyset分页替代逐项哈希查询及重复COUNT；相似排名集合最多150项、原排序不变；收藏与批量标签使用源资源账号。完整QA 67通过/0失败/1忽略，all-targets/static/diff通过。500项SQLite夹具测试分页、账号、排序和扫描中止。单次阻塞I/O不能保证立即取消，派生索引不回滚；未做新的视觉及Release性能测量。V0.7固定候选及发布门禁不变。

## 后续设计第五批：缩略图调度与缓存生命周期
基于1e723b8继续同一PR。ThumbnailModel Entity 独立任务和完成尝试，追加不取消、重置取消、离开暂停/返回恢复、事件代际双检查；详情路径复用、比例更新及账号切换解码缓存释放。151项隔离资源、真实GPUI绘制后缓存清零及永久微缩图/原文件保留回归；完整QA 68通过/0失败/1忽略，all-targets、QA build、static/diff通过。7张实际渲染截图并审查其中3张。当前单项阻塞操作不能保证立即中止，未做长期资源/Release性能、Windows真机。冻结V0.7候选及发布门禁不变。

## 后续设计第六批：原生详情焦点与 Release 性能复验
- b00471f CI 45547718069 success，PR #8 继续集中开发。固定详情头部、窄窗详情/查看器/图库焦点路径、分类切换及窗口缩窄修正。68 QA通过/0失败/1忽略；all-targets、QA debug/release build、static/diff通过；8张实际截图，3张详情图人工审查。Windows真机仍阻塞。
- macOS ARM64，10,000项合成编码JPEG，150项分页，Release QA同脚本短程滚动；首轮与视觉采集部分重叠，独立重跑作为参考而非因果证据。

| 模式与指标 | 旧基线 | 首轮 | 独立重跑 |
| --- | ---: | ---: | ---: |
| Grid CPU绘制p95(ms) | 2.260 | 4.829 | 2.064 |
| Masonry CPU绘制p95(ms) | 1.775 | 4.295 | 1.577 |
| Grid采样峰值RSS(MiB) | 133.55 | 147.86 | 138.47 |
| Masonry采样峰值RSS(MiB) | 133.39 | 148.25 | 143.14 |
| Grid首屏(ms) | 142 | 188 | 103 |
| Masonry首屏(ms) | 115 | 104 | 100 |

重跑 grid 600项/masonry 450项，解码缓存均192/192、无超时、native callback p95 20.029/20.030ms（非显示FPS）。全部JSON及截图在dist/design-research/focus-after/。RSS仍高于旧基线，下一轮应对长程滚动、账号切换、资源回收进一步测量，不能将短程数据作为泄漏证明或性能收益保证。未清系统缓存、未独占机器，保留所有原始结果；不修改冻结V0.7候选及发布门禁。

## 后续设计第七批：原生纹理资源回收与滚动诊断
84bab5e 双平台CI 45548753872 通过，继续 PR #8。App::defer修复当前窗口批量drop_image遗漏风险，离开图库清空解码，返回按需解码；释放重置容器容量和旧布局引用。完整QA 68通过/0失败/1忽略，all-targets、Release QA、静态/Python语法通过。
10k合成JPEG、每模式3×180次滚动，27检查点/模式；原始JSON与日志在dist/design-research/memory-after/。独立进程短程多循环，非真实凭据、非用户数据。

| 指标 | 网格 | 瀑布流 |
| --- | ---: | ---: |
| 每轮加载资源 | 1050 | 2250 |
| 峰值采样RSS MiB | 140.16 | 155.42 |
| 结束滚动缓存 | 192 | 192 |
| 离开图库缓存 | 0 | 0 |
| 返回图库缓存 | 20 | 24 |
| 重置后资源数 | 150 | 150 |
| 重置后缓存 | 25 | 18 |
| 运行秒数 | 22.399 | 22.304 |
| RSS采样数 | 102 | 105 |

重置阶段最近RSS采样（MiB）：grid 119.58/75.73/91.39；masonry 137.97/140.45/140.69。三轮未见持续快速累计，瀑布流仍有小幅增长，不能排除长期泄漏、分配器保留或驱动占用；无旧版同长程对照，不宣称修复带来确定RSS降幅。V0.7冻结及Windows真机门禁不变。

## 后续设计第八批：瀑布流布局增量计算
9594ac9 TLS阻塞已恢复并推送，CI 45549944690开始运行。瀑布流列状态复用，追加只布局新增项；真实最大高度裁剪、日期二分、锚点优化、行导航减少临时分配。10k分批等价与超高磁贴可见性测试，69 QA通过/0失败/1忽略，all-targets、Debug QA build、static/diff通过；780×640真实瀑布流截图审查通过，dist/design-research/layout-after/。首次夹具宽度490px紧凑双列，改480px单列覆盖超高边界。未重测Release/RSS，不作端到端性能保证。保持冻结发布候选、原数据与依赖不变。

## 后续设计第九批：行布局增量与分页反馈
0dac367已推送，9594ac9双平台CI通过，0dac367 CI 45551785937运行中。列表/等宽网格/等高拼图增量重排末行及后缀，保留日期标题、原生ListState前缀与选择；追加不失效已知比例，旧比例变化或整页替换完整重排。无效比例回退，追加加载/错误固定底栏反馈。10k资源三布局×双密度×日期模式等价测试与现有原生滚动回归通过；完整QA 70通过/0失败/1忽略，all-targets、QA Debug build、static/diff通过。深色拼图/浅色网格实际截图已审查，dist/design-research/row-after/。未重测Release/RSS，未独立量化加载态布局位移，不宣称端到端性能提升。冻结候选及Windows真机发布门禁不变。

## V0.8.0 开发候选收口
基于ef31fce，CI 45552122688 双平台通过。修复窄窗分页错误操作冲突，固定底栏、隐藏重复加载与非关键控件，保留重试及完整错误提示；真实GPUI 720×600几何与重试测试通过。版本元数据、README、CHANGELOG同步0.8.0开发候选。完整QA 71通过/0失败/1忽略、all-targets/static/diff、Release QA构建通过；远程连接短暂中断后恢复并核实最终结果。
## 本轮 Release QA 复验
- macOS ARM64 新版本 Release QA 构建通过；11张实际渲染截图已保存，人工审查720×600图库、任务、设置与详情4张，未发现明显控件截断；设置显示v0.8.0。其余截图仅完成生成，不宣称全量人工验收。
- 10k合成JPEG、同脚本顺序运行，两个布局均未超时，缓存上限192。不是10k全部载入、真实显示FPS或长时泄漏证明。未独占机器、未刷新系统缓存；不对单次指标宣称确定性能收益。

| 布局 | 首屏(ms) | CPU绘制p50(ms) | p95(ms) | 峰值RSS(MiB) | 已加载 | 解码缓存 |
|---|---:|---:|---:|---:|---:|---:|
| grid | 120 | 2.292 | 4.888 | 145.5 | 600 | 192/192 |
| masonry | 122 | 1.420 | 1.643 | 130.2 | 450 | 192/192 |

证据目录：dist/design-research/v0.8-closeout/（本地生成，不纳入Git）。上一轮 ef31fce CI 45552122688 已通过；本轮最终提交仍需独立CI。Windows真机及正式发布继续阻塞。

## 最终固定候选与原始 CI 产物（2026-11-25）

- 固定代码候选：aec2446fa46c172840f38066e87e564c6422799e；PR #8，CI 45553650482（headSha 一致，双平台 success）。之后仅允许验收文档更新，不改变此候选的源代码和原始产物。
- 原始 Windows x64 ZIP 9439257 bytes、macOS ARM64 DMG 9040235 bytes，及各自原始 SHA256 文件均从该次 CI 下载，保存于 dist/release-audit-v0.8.0/ci-artifacts/；四项完整摘要见 [v0.8.0-verified-assets.json](v0.8.0-verified-assets.json)。校验前后均比对原始摘要，不重新打包或覆盖校验文件。V0.7 四项原始文件也重新核对，保持不变。
- Windows：ZIP CRC、x64 PE、图标/版本资源、INSTALL.txt 和原始 SHA256 通过；CI 原生 ProductName/ProductVersion 检查通过。实际 Windows 设备仍离线，不宣称运行、托盘或文件选择验收通过。
- macOS：复用 dist/dmg-venv，原始 DMG CRC、只读挂载、ARM64、应用版本/图标/标识、ad-hoc 签名、Finder 安装布局通过。隔离复制后实际启动，Demo SQLite 初始化为空、1160×828 窗口、DESKTOP tray=true hotkey=true；测试进程已退出。默认 Python 缺少 ds_store，切换既有打包虚拟环境后通过；hdiutil 弃用及 sandbox_extension 警告不阻塞本次启动。未安装到 Applications、未验证 Gatekeeper 公证、未触发物理快捷键/真实上传。
- 完整验收日志与 smoke.json 保存在 dist/release-audit-v0.8.0/。正式版本仍为 v0.5.1。V0.7 固定候选优先收口，Windows 真机门禁未完成，不合并 PR #8，不创建 Tag/Release，不改变发布顺序。
