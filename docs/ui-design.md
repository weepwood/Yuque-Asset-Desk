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
