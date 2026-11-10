# Natural Skin Native

面向 PhotoCraft 的原生磨皮伴随程序。**首版按 Windows/macOS 共用的 Rust + egui 界面开发**，不修改 PhotoCraft，也不再维护 WASM 版本。

当前桌面开发版 **0.3.0** 新增 PhotoCraft 风格外观、独立中英文界面、1:1 细节预览、SDR ICC 转换和分块导出。支持连接设置、单实例唤回、可选登录启动和结果找回；普通启动无需命令行参数。它还提供：打开单层 `.pcraft` 或 RGB TIFF、后台预览、原图/结果切换、皮肤蒙版、参数调节、取消、阶段及耗时显示、全分辨率导出新 `.pcraft`。预览基础缓存在进程内复用，连续拖动参数时丢弃过期结果。

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

窗口顶部可在 **语言** 中选择 **简体中文 / English**，即时切换并独立保存，重启后保留，不改变 PhotoCraft 设置。首次语言按系统选择中文或英文；旧版连接配置自动兼容。**外观** 提供 PhotoCraft 风格的深色、灰色、浅色及跟随系统选项。

第一次打开后进入 **连接设置（Connection）**：

1. 选择 PhotoCraft 应用，点击 **启用连接（Enable connection）**。程序会保存设置并生成动作安装脚本。
2. 在 PhotoCraft 中通过 **File → Scripts → Browse** 导入页面所示的 `Install Natural Skin.json`，只需一次。在动作面板中找到 **Natural Skin Send**。
3. 在 PhotoCraft 中选择普通像素图层，运行该动作。Natural Skin 会接收图层快照并打开预览。
4. 调参后点击 **应用到 PhotoCraft（Apply to PhotoCraft）**，全分辨率处理完成后在 PhotoCraft 打开一个新文档。需要另存时使用 **导出结果（Export Result）**。

关闭窗口会最小化并继续接收；**Quit** 完全退出。也可自行勾选 **Start at login**，默认关闭。重开程序会恢复连接，重复启动会唤回已有实例。回传结果保留在程序数据目录；**Connection → Open last result / Show saved files** 可找回。没有自动清理结果，请保存需要的文档后自行管理磁盘空间。

也可以跳过连接设置，直接打开支持的 `.pcraft` 或 RGB TIFF。详细说明见 [0.2.0 桌面使用与验收](docs/desktop-0.2.zh-CN.md)；早期路线证据见 [动作联动验证](docs/handoff-route-2026-11-09.zh-CN.md)。

macOS `.app` 是本地开发签名，未做发行签名/公证。Windows 已通过目标平台编译检查，但尚未在真实 Windows 机器运行验收；没有把检查结果当作 Windows 安装包。

## 当前能力与范围

- 原生 `.pcraft` 保留 8/16/32 位 RGB/RGBA、Alpha、透明像素下的 RGB、ICC、DPI、画布外图块和边缘图块数据。校验 ZIP、BLAKE3 和资源预算；生成完成后才发布新文件，不覆盖已有输出。
- 首版仅接收一个可见的普通顶层像素图层，Normal、100% 不透明度/填充，无选区、附加通道、蒙版、剪贴、图层效果或时间线。复杂文档会明确拒绝。
- Fit 预览长边最多 1024；新增可拖动或用方向键平移的 1:1 原分辨率局部预览。完整处理最多 36 MP，按块导出以限制滤波工作区。Fit 的半径/边缘保护变化重建基础缓存，强度/纹理/皮肤范围/调色复用缓存。
- 预览按缩放比例调整半径，但缩小图上的滤波仍是近似，导出始终从完整原图重新计算。皮肤蒙版只用于查看，导出结果不会变成蒙版。
- 支持 SDR RGB matrix/TRC ICC → sRGB 工作空间 → 源 ICC 的处理，以及手动选择显示输出 ICC；未标记图像按 sRGB。无效、LUT 和 PQ/HLG 配置明确拒绝。未完成真实显示器、自动显示配置切换或 HDR 认证；不提供 ETA。
- 原生 TIFF 自身读写可保留透明像素数据；PhotoCraft 的平面 TIFF 导出会丢失完全透明像素下 RGB，因此优先使用 `.pcraft`。

本轮局部预览、色彩与性能范围见 [实现与验收](docs/quality-color-performance-2026-11-10.zh-CN.md)。暂时没有授权真人照片，效果验收仍限于合成样本。

## 命令行后端

```sh
./dist/natural-skin-companion process input.pcraft output.pcraft \
  '{"Strength":45,"Radius":6,"Texture":60,"skinBrightening":20}'
./dist/natural-skin-companion --help
```

阶段/已用时间写入 stderr，完整文件写完后才报告完成。测试和复现见 [测试方法](docs/testing.md)。旧 WASM 数据仅作为 [历史记录](docs/history/wasm/README.zh-CN.md)。

[MIT](LICENSE)
