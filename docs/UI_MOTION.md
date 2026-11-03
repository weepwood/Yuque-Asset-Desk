# UI 与动效规范（v0.2.1）

## 设计目标
桌面原生轻量工具；上传入口一眼可见，历史资源和链接复制无需翻页，保持 macOS / Windows 一致的空间结构。

- 视觉：深蓝灰 #0B1320 / #101D2E，亮蓝强调 #377CEF；输入、操作卡片使用 10–16px 圆角。
- 布局：233px 导航栏，顶部状态指示，上传 Hero、资源/队列卡片，底部可搜索的历史资源区域。
- 主操作：文件选择器（拖拽和粘贴截图尚未实现）；上传不在 UI 线程进行。
- 列表：鼠标经过行时显示明亮轮廓，复制原始 URL 或 Markdown；筛选图片为纯 UI 层过滤。
- 空状态：无上传记录时展示引导而非空白区域。

## 动效时长与触发

| 元素 | 动作 | 时长 | 触发条件 |
| --- | --- | --- | --- |
| 上传区域 | 一次淡入 | 320ms | 主窗口首次启动 |
| 原生按钮 | 背景/边框颜色 | 140ms | hover / press |
| 侧边导航 | 背景颜色 | 170ms | hover / active |
| 历史列表 | 背景/边框颜色 | 130ms | hover |
| 状态指示灯 | 背景颜色 | 160ms | 上传状态变化 |

只使用 Slint 内置动画属性，不调用 animation-tick()，没有无限重复动画、模糊材质或额外 WebView。上传空闲后没有由本项目主动驱动的持续刷新。

## 性能验收
- 无任何待上传文件时，确保 Rust worker 阻塞等待 mpsc channel。
- 上传任务中的数据读取采用流式 multipart；不缓存图片像素数据。
- 分别检查 Windows x64、macOS ARM64 / x64 上启动/空闲/100 个历史条目的内存及响应速度。
- 使用 `SLINT_DEBUG_PERFORMANCE=refresh_lazy,console` 观察空闲刷新；使用 `SLINT_SLOW_ANIMATIONS=4` 逐项检查动画。
- 以 GitHub Actions 编译结果为最低门槛，正式发布前需要在真机上进行键盘可用性、系统托盘、DPI 缩放和点击目标回归测试。

参考：
- https://docs.slint.dev/latest/docs/slint/reference/language/animations/
- https://docs.slint.dev/latest/docs/slint/reference/window/systemtrayicon/
