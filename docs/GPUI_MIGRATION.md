# GPUI Kit UI 迁移设计

## UI 技术结构

- 单 crate 依赖 `gpui-kit = "=0.9.0"`，原生 GPU 渲染。
- `Dashboard` 是持久 GPUI Entity，`Render` 输出导航栏、上传区域、统计卡片和历史资源。
- `InputState` 配合订阅更新搜索输入，按钮使用 GPUI Kit Component。
- `cx.background_spawn` 执行文件上传，完成后回到 GPUI 前台 Context 更新状态。
- 队列上限 128，每次最多一个同步文件上传任务，不创建无限后台任务或 UI 定时刷新。

## 动效原则

目前使用 GPUI Kit 主题组件自带的 hover/press/focus 反馈，不新增持续 GPU 动画。
后续可在 GPUI Base Motion 上实现 160ms-240ms 过渡；任何动画必须尊重 reduced-motion，并在 Windows/macOS 真机测帧时间。

## 注意

- macOS 15+ 是当前 GPUI Kit 的官方要求，不能承诺更早系统兼容。
- GPUI Kit 与 Slint 有不同的系统托盘集成方式，托盘需要后续另行移植。
- 语雀账号上传并非官方稳定 OpenAPI，不能因为 UI 已迁移而默认已通过实测。
- 旧版 v0.2.1 的数据库保持兼容，建议更新前备份。

文档：https://gpui-kit.com/docs/getting-started/
