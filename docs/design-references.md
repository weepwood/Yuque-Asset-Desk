# 设计研究来源（固定提交）
实际读取 GitHub 原始文本；完整下载副本在本地 ignored dist/design-research/。不复制第三方实现。

- [linear](https://github.com/VoltAgent/awesome-design-md/blob/fcc70284932276d085f01ef9f83edde90b1258cb/design-md/linear.app/DESIGN.md) — `fcc70284932276d085f01ef9f83edde90b1258cb`
- [raycast](https://github.com/VoltAgent/awesome-design-md/blob/fcc70284932276d085f01ef9f83edde90b1258cb/design-md/raycast/DESIGN.md) — `fcc70284932276d085f01ef9f83edde90b1258cb`
- [spaceui](https://github.com/spacedriveapp/spaceui/blob/a48b17047d3ff1d05268acb6f88a79460ad4fa7b/packages/explorer/README.md) — `a48b17047d3ff1d05268acb6f88a79460ad4fa7b`
- [spaceui-root](https://github.com/spacedriveapp/spaceui/blob/a48b17047d3ff1d05268acb6f88a79460ad4fa7b/README.md) — `a48b17047d3ff1d05268acb6f88a79460ad4fa7b`
- [immich](https://github.com/immich-app/immich/blob/56074c0ec76e53e2e0cfc3f32caba7e0e96e973d/README.md) — `56074c0ec76e53e2e0cfc3f32caba7e0e96e973d`
- [gpui-design](https://github.com/longbridge/gpui-kit/blob/c11ce78b0c8a70eecca27bc226f0fc3bfa3b52ff/skills/gpui-kit-design-guides/references/design-guides.md) — `c11ce78b0c8a70eecca27bc226f0fc3bfa3b52ff`
- [gpui-coding](https://github.com/longbridge/gpui-kit/blob/c11ce78b0c8a70eecca27bc226f0fc3bfa3b52ff/skills/gpui-kit/references/coding-guides.md) — `c11ce78b0c8a70eecca27bc226f0fc3bfa3b52ff`
- [Immich AssetViewer](https://github.com/immich-app/immich/blob/56074c0ec76e53e2e0cfc3f32caba7e0e96e973d/web/src/lib/components/asset-viewer/AssetViewer.svelte)
- [Immich AssetViewerNavBar](https://github.com/immich-app/immich/blob/56074c0ec76e53e2e0cfc3f32caba7e0e96e973d/web/src/lib/components/asset-viewer/AssetViewerNavBar.svelte)
- [Immich NavigationArea](https://github.com/immich-app/immich/blob/56074c0ec76e53e2e0cfc3f32caba7e0e96e973d/web/src/lib/components/asset-viewer/NavigationArea.svelte)
- [Immich DetailPanel](https://github.com/immich-app/immich/blob/56074c0ec76e53e2e0cfc3f32caba7e0e96e973d/web/src/lib/components/asset-viewer/DetailPanel.svelte)

## 设计决策
SpaceUI FileGrid 的 onFileClick/onFileDoubleClick/selectedIds 分离，Inspector 只消费选中对象；Linear/Raycast 是社区官网规范（不等同产品官方规范），只取 surface ladder、语义色、键盘命令表达。Immich viewer 独立导航与详情状态。GPUI guide 的 stable shell、desktop command surfaces、explicit focus、async side effects、retained Entity 为工程约束。
## 现状证据
5410fc2 实际 QA 截图复制到 dist/design-research/baseline，SHA256 清单 baseline.sha256。当前图墙顶部重复标题/数量、工具栏和底部重复缩放、选择改变布局；历史文档不是可执行规范。

## 实际参考图
- Spacedrive: https://github.com/spacedriveapp/spacedrive/blob/d82b1ce07ffd6352585cdefc6ed82e50f8319fab/docs/public/SDGridView.webp 。实际下载并审查；独立工具栏，文件名和属性分层，窄而稳定的导航/检查器，真实图片不填满灰色容器。
- Immich: https://github.com/immich-app/immich/blob/56074c0ec76e53e2e0cfc3f32caba7e0e96e973d/design/immich-screenshots.png 。实际下载并审查，图库内容优先，查看器导航和详情独立。仅参考，不纳入发布素材。
6298fc66795e48d58b7fdc441dd16817b4e1720016020e3b58e872b09672598b  dist/design-research/immich-reference.png
683bfea7e58e9b94483d5fc44dd2888416ead1fb6aa95f242b5aa58a7ed88aba  dist/design-research/spacedrive-reference.webp
