# V0.5.0 桌面 UI 设计与组件审计

## 设计依据
- GPUI Kit 0.9.0 锁定版本： https://github.com/longbridge/gpui-kit/releases/tag/v0.9.0 ，以 Cargo registry 实际源码为 API 权威；核对 menu/dropdown_menu.rs、menu/context_menu.rs、sidebar/menu.rs、theme/mod.rs、progress/progress.rs。
- 官方设计指南：https://github.com/longbridge/gpui-kit/blob/main/skills/gpui-kit/references/coding-guides.md 。复用已有控件及主题更新机制，不以通用 div 重做菜单。
- Eagle 的工作台导航、工具栏、图片列表结构：https://en.eagle.cool/support/get-started 。借鉴内容优先、低频功能渐进披露；不复刻品牌、图片或受许可限制的源代码。
- Finder/Linear 作为紧凑导航、弱分隔、单一主操作和信息层级的视觉参考，不宣称像素级复刻。
- GPUI Kit 保持现有锁定依赖。无需新组件库，不引入 WebView。

## 复用与边界
| 控件 | 实施 |
|---|---|
| 侧栏导航/标签节点 | SidebarMenuItem，标签路径业务层保留 |
| 筛选/布局 | DropdownMenu + PopupMenuItem，原生键盘和焦点处理 |
| 卡片右键 | ContextMenuExt + PopupMenuItem |
| 进度 | Progress，明确是请求体读取进度 |
| 基础按钮/输入/提示/图标/滚动 | 继续复用 Button / Input / Tooltip / Icon / Scrollbar |
| 图片墙 | 保留项目专用虚拟列表、等高拼图、Masonry 算法，192 项缓存 |
| 设置 | 自定义业务布局，复用输入、按钮，限制宽度 |

## 视觉令牌
入口 src/desk/design.rs。背景、侧栏、surface、subtle、line、text、muted、accent、accent_soft、error 十个语义角色；深浅主题映射，Theme::update 同步组件层颜色。绿色强调用于活动导航、收藏、选中与成功状态，避免所有控件着色。
侧栏 184/150px，主内容内边距 16px，控件圆角 6px，图片卡片名栏 24px；标题 text_lg/semibold、正文 text_sm、辅助 text_xs/muted。状态反馈过渡 160ms，复用 GPUI 动效和减少动效设置；不额外引入动画依赖。
图片优先，默认无选中边框、无常驻操作栏。保留文件名与收藏标识，悬停/选中显示选择、收藏、复制 URL；Markdown、详情、相似检索放右键菜单。保留原图宽高比和本地 160px JPEG，不自动远端取图。原始默认全部资源保留，防止视频/附件被误隐藏，图片独立导航。
设置页集中主题、快捷键、备份与账号，任务页保留安全操作和独立入口。筛选/布局菜单收纳低频操作；窄窗口搜索独占一行。详情按需，沉浸模式可退出。

## V0.5.0 精修规范
- 标题 18px、工作台正文 13px、图片文件名 11px；图片间距 10px、日期标题 28px、详情栏 280px。深浅主题统一主按钮/浮层语义色，图标区分图片、视频、附件、布局与筛选。
- 原生 Slider 用于图片密度 96–240px（4px 步进），拖动更新视图、释放持久化；仍保留 +/- 快捷调节，双向同步。布局计算与绘制共用 TILE_GAP/DAY_H，避免锚点漂移。
- 原生 Switch 控制深色主题，快捷键使用 DropdownMenu，Empty 提供空相册引导，复制反馈使用应用内 Notification（不触发系统通知）。完整 Settings 容器会重复应用导航，因此保持单层设置布局。
- 卡片悬停渐变，选中轮廓独立叠加，不改变图像测量；详情优先预览/复制，技术属性下移。窄窗详情使用明确剩余宽度，工具栏按剩余宽度换行；放大预览按实际画布宽高与图像比例计算适应尺寸，100% 表示适应窗口。
- Theme motion 快速/常规/慢速 120/180/240ms，详情淡入用现有 transition；菜单、Switch、Slider 保持库自身交互与减少动效机制。不开启常驻动画循环，不宣称实际帧率提升。未全面重做侧栏位移动效。

## 舒适与紧凑
默认舒适模式，设置及布局菜单可切换。主内容边距 24/16px，图片间距 16/10px，日期标题 36/28px，区块 16/8px，任务/列表内边距 16/12px，导航间隔 6/4px。两种主题和原图片尺寸设置独立。详情低频操作采用轻量按钮，上传恢复入口收纳到底栏，实际上传时显示批次与文件读取反馈。保留原生组件和虚拟化布局。

## V0.5.1 舒适模式二次精修
舒适/紧凑模式分别采用主内容边距 32/16px、图片间距 24/10px、日期标题 44/28px、区块间距 22/8px、列表内边距 20/12px、导航间隔 10/4px、图名区域 32/24px。宽窗侧栏 200/184px、详情栏 320/280px，窄窗侧栏保持 150px。主标题 21/18px、正文 14/13px、文件名 12/11px。舒适模式标题独立成行，使用原生 Medium 操作按钮，搜索与按钮响应式换行，底部状态和工具紧密归组；窄窗品牌缩写为 Yuque。详情预览 208/184px，名称与属性、账号标签与输入框使用局部小间隔。紧凑模式保留原间距、字号与控件规格，工具栏允许换行防溢出。不调整用户图片尺寸、主题或偏好结构。

## V0.5.1 原生动效与交互精修
参考 [PicView](https://github.com/Ruben2776/PicView) 的图片优先/虚拟图库（GPL，仅借鉴设计原则）、[Zed dock](https://github.com/zed-industries/zed/blob/main/crates/workspace/src/dock.rs) 的工作区面板层级和 [GPUI Kit motion](https://github.com/longbridge/gpui-kit/blob/main/docs/STYLING-AND-MOTION.md)。实际实现以本地锁定 gpui-base 0.9.0 为准；不复制第三方代码或素材，不增加依赖。
- 舒适模式缩放按钮组合、筛选及沉浸模式选中状态，日期标题在网格/瀑布流对齐，详情收藏和相似图片操作同组并支持换行。
- design::SurfaceMotion：主工作区六种页面的 180ms 0.88→1 淡入，初始显示不动画，切换才启动；不阻塞事件。详情使用 180ms 原生值过渡淡入；关闭立即移除，避免不可见控件仍接收点击。任务页不再展示旧详情。
- 仅虚拟列表可见及 overscan 图片使用稳定 asset ID 的 120ms 悬停渐变，反向使用当前插值；控件退出立即移除，避免幽灵点击。预览每次打开独立缩放通道，160ms ease-out 数值插值，拖动即时，关闭不继续运行隐藏动效。
- 使用 GPUI Kit 本身的 frame scheduling 和 OS reduce_motion，不使用循环计时器/全图入场/宽度渐变，不强制高帧率。已锁定的组件自带菜单、按钮、Switch 等交互，不另造动效框架。

## V0.5.1 面板、浏览及设置专项
参考 [Zed 面板系统](https://zed.dev/blog/new-panel-system)、[PicView](https://github.com/Ruben2776/PicView) 及 [GPUI Kit motion](https://github.com/longbridge/gpui-kit/blob/main/docs/STYLING-AND-MOTION.md)。使用现有 GPUI 原生控件和 motion 通道，无第三方代码移植或新增依赖。
- 圆角统一为 8px，设置页以细边框区分更新、外观、存储及账号，窄窗品牌缩写为“语雀”，沿用原舒适/紧凑字体、间距参数。菜单、Tooltip、通知、系统确认及文件选择仍使用原生实现，未另造浮层或修改安全确认逻辑。
- 侧栏可收起为 56px 图标栏，保留资源、任务与设置入口，标签需展开。会话内状态，不新增持久化字段。侧栏展开/收起采用短距离滑入，详情打开采用 14px 滑入及淡入，沉浸模式切换纳入工作区淡入；布局宽度在状态切换时更新，不逐帧重排图库，详情关闭立即移除，未做完整退出空间动画。减少动态效果时立即呈现。
- 图片双击预览，单击立即选择；当详情布局将变化时延迟 500ms 打开详情以保留双击目标。预览独立键盘焦点，左右切换已加载图片、+/- 缩放、0 复位、Esc 返回；滚轮以当前指针为锚点直接缩放，按钮保持原 160ms 插值，拖动时中断缩放并限制实际图片边界。没有自动跨页读取或下载原图。
