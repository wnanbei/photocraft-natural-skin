# Natural Skin Native

面向 PhotoCraft 的原生磨皮伴随程序。**首版按 Windows/macOS 共用的 Rust + egui 界面开发**，不修改 PhotoCraft，也不再维护 WASM 版本。

当前桌面开发版 **0.2.0** 支持连接设置、单实例唤回、可选登录启动和结果找回；普通启动无需命令行参数。它还提供：打开单层 `.pcraft` 或 RGB TIFF、后台预览、原图/结果切换、皮肤蒙版、参数调节、取消、阶段及耗时显示、全分辨率导出新 `.pcraft`。预览基础缓存在进程内复用，连续拖动参数时丢弃过期结果。

[English](README.md) · [本轮开发与验证](docs/native-development-2026-11-08.zh-CN.md) · [最初可行性研究](docs/native-feasibility-2026-11-08.zh-CN.md) · [开发顺序](docs/native-roadmap.zh-CN.md)

## 构建和使用

需要 Rust **1.95+**。首次获取锁定依赖后可离线构建，无需 Node.js、Python、模型或云服务：

```sh
cargo fetch --locked
sh build.sh
```

macOS 打开 `dist/Natural Skin.app`；Windows 在本机运行 `build.ps1`，打开 `dist/natural-skin.exe`。也可以直接运行：

```sh
cargo run --release --features gui --locked --offline --package natural-skin-companion --bin natural-skin
```

第一次打开后进入 **Connection**：

1. 选择 PhotoCraft 应用，点击 **Enable connection**。程序会保存设置并生成动作安装脚本。
2. 在 PhotoCraft 中通过 **File → Scripts → Browse** 导入页面所示的 `Install Natural Skin.json`，只需一次。在动作面板中找到 **Natural Skin Send**。
3. 在 PhotoCraft 中选择普通像素图层，运行该动作。Natural Skin 会接收图层快照并打开预览。
4. 调参后点击 **Apply to PhotoCraft**，全分辨率处理完成后在 PhotoCraft 打开一个新文档。需要另存时使用 **Export Result**。

关闭窗口会最小化并继续接收；**Quit** 完全退出。也可自行勾选 **Start at login**，默认关闭。重开程序会恢复连接，重复启动会唤回已有实例。回传结果保留在程序数据目录；**Connection → Open last result / Show saved files** 可找回。没有自动清理结果，请保存需要的文档后自行管理磁盘空间。

也可以跳过连接设置，直接打开支持的 `.pcraft` 或 RGB TIFF。详细说明见 [0.2.0 桌面使用与验收](docs/desktop-0.2.zh-CN.md)；早期路线证据见 [动作联动验证](docs/handoff-route-2026-11-09.zh-CN.md)。

macOS `.app` 是本地开发签名，未做发行签名/公证。Windows 已通过目标平台编译检查，但尚未在真实 Windows 机器运行验收；没有把检查结果当作 Windows 安装包。

## 当前能力与范围

- 原生 `.pcraft` 保留 8/16/32 位 RGB/RGBA、Alpha、透明像素下的 RGB、ICC、DPI、画布外图块和边缘图块数据。校验 ZIP、BLAKE3 和资源预算；生成完成后才发布新文件，不覆盖已有输出。
- 首版仅接收一个可见的普通顶层像素图层，Normal、100% 不透明度/填充，无选区、附加通道、蒙版、剪贴、图层效果或时间线。复杂文档会明确拒绝。
- 预览最多长边 1024，最多 2 MP；完整处理最多 36 MP。半径/边缘保护变化重建基础缓存，强度/纹理/皮肤范围/调色复用缓存。
- 预览按缩放比例调整半径，但缩小图上的滤波仍是近似，导出始终从完整原图重新计算。皮肤蒙版只用于查看，导出结果不会变成蒙版。
- 当前显示按 sRGB 使用；ICC 数据保留，但广色域/HDR 磨皮和显示色彩管理尚未认证。大区域滤波的取消仍有等待时间，尚无剩余时间估算。
- 原生 TIFF 自身读写可保留透明像素数据；PhotoCraft 的平面 TIFF 导出会丢失完全透明像素下 RGB，因此优先使用 `.pcraft`。

## 命令行后端

```sh
./dist/natural-skin-companion process input.pcraft output.pcraft \
  '{"Strength":45,"Radius":6,"Texture":60,"skinBrightening":20}'
./dist/natural-skin-companion --help
```

阶段/已用时间写入 stderr，完整文件写完后才报告完成。测试和复现见 [测试方法](docs/testing.md)。旧 WASM 数据仅作为 [历史记录](docs/history/wasm/README.zh-CN.md)。

[MIT](LICENSE)
