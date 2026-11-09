> 历史 WASM 版本记录：已停止维护，当前开发目标为原生伴随程序。

# Natural Skin 自然肌肤

[English](README.md) · 简体中文

给 PhotoCraft 使用的保留边缘磨皮滤镜。发布版的可安装 `.wasm` 文件位于 [GitHub Releases](https://github.com/wnanbei/photocraft-natural-skin/releases)，也可用源码构建。使用已编译插件不需要 Python、Rust、Claude、模型下载或网络连接。

已在 macOS **PhotoCraft 0.20.2（7c12f6471）自带的命令行引擎**中实际安装、执行和导出验证。当前 1.1.1 构建也通过了 **PhotoCraft 0.21.0** 的 CLI 集成验证。接口为 PhotoCraft WebAssembly ABI v1；同一文件按接口设计可用于 macOS、Windows、Linux 和网页构建，但本次没有在 Windows/Linux 或网页端实测，也没有验证 GUI 的实时预览性能。

CI 使用单个 Ubuntu 任务完成编译、测试和打包，生成一份跨平台 `.wasm` 文件。各平台 PhotoCraft 的实际兼容性仍需在相应主程序中验证。

## 安装

1. 把 `NaturalSkin-<版本>.wasm` 保存到一个固定文件夹，例如 `NaturalSkin-1.0.0.wasm`。
2. 在 PhotoCraft 打开 **Filter（滤镜）→ Plug-ins（插件）→ Install Plug-in…（安装插件）**，选择这个 `.wasm` 文件。
3. 在 **Filter → Plug-ins** 中找 **Natural Skin**。1.0.1 起插件自带的名称和说明统一使用英文，避免插件自身的文案中英混排。

PhotoCraft 0.20.2 的插件加载按进程保存。为了下次启动仍然能找到它，进入 **Edit → Preferences → Plug-ins**，启用 **Use Additional Plug-ins Folder**，把 **Additional Plug-ins Folder** 指向第 1 步保存插件的文件夹。`.wasm` 应直接放在该文件夹内，不要放在更深一层子文件夹。

上述入口来自 [PhotoCraft 0.20.2 官方插件说明](https://github.com/storytold/photocraft/blob/v0.20.2/docs/plugins.md)。不同界面语言可能使用不同译名；英文名称可用于对照。

## 第一次使用

1. 打开 RGB 照片，复制照片图层，在副本上处理。
2. 选中照片的像素图层缩略图；不要选图层蒙版、Alpha 通道或快速蒙版。
3. 用套索等工具选出要处理的皮肤，适当羽化边缘。没有选区时会处理整个图层中符合条件的像素。
4. 执行 **Natural Skin**，先用默认值：`Strength=45`、`Radius=6`、`Edge_Protection=70`、`Texture=60`。
5. 如需查看处理范围，打开 `Show_Mask`：白色表示较强作用范围，黑色表示不处理，中间灰色表示部分作用。
6. **正式磨皮前把 `Show_Mask` 关闭**。保持开启并确定会把灰度遮罩预览写进照片；这是调试显示模式，此时可撤销。
7. 确定后比较副本与原图，需要时降低副本图层的不透明度。每次滤镜执行可用一次 Undo 撤销。

本滤镜直接平滑像素，不会自动创建高低频图层组，也不瘦脸、不改变五官。

## 更快的两步操作

1. **先磨皮**：将美白 `skinBrightening` 和红润 `skinRosiness` 都设为 0，调整磨皮参数后点击确定。
2. **再调肤色**：在已经磨皮的同一图层上重新打开插件，把 `Strength` 明确设为 **0**，关闭 `Show_Mask`，再调美白和红润后确定。

第二步使用已经处理过的照片作为输入，只做逐像素调色，不重复计算磨皮，也不申请滤波工作缓存。仅在第一次面板中将 `Strength` 改成 0，并不会保留尚未确认的磨皮结果。两步分别产生一次可撤销操作；动作或智能滤镜也应使用前后两个处理步骤。

1.1.1 会裁去输入图块中未命中的边缘，并在皮肤分散时分别处理各区域，跳过中间的大块背景；仍保留两倍半径的上下文和非肤色邻居的统计作用。工作缓存按最大区域分配并重复使用，无命中时不分配。密集、细碎或预计不划算的区域自动走原路径。`Texture=0` 时跳过纹理保护的邻域计算。收益取决于每个主程序图块内的肤色分布，大图不保证提速。详见[本版区域优化实测与验证](skin-region-performance-1.1.1.zh-CN.md)。

## 参数对照

插件面板由 PhotoCraft 自动生成，并按主程序的界面语言翻译它认识的标签。例如，中文界面会将 `Radius`、`Texture` 翻译成“半径”“纹理”，但其他未收录的标签仍显示英文。PhotoCraft 0.20.2 的 ABI v1 没有独立的显示名称或插件级语言控制接口，因此仅修改插件无法保证整个窗口使用同一种语言。本版将插件自带名称和说明统一为英文，保留原有参数键、默认值和插件 ID，兼容已有的动作与智能滤镜设置。需要整个窗口统一英文时，应将 PhotoCraft 的界面语言设为英文；统一中文则需要主程序支持完整翻译。下面的对照表解释每个参数。

| 参数 | 中文含义 | 范围 / 默认值 | 调整建议 |
|---|---|---|---|
| Strength | 磨皮强度 | 0–100 / 45 | 先用 30–50。0 关闭磨皮，美白和红润仍可独立使用。 |
| Radius | 作用半径 | 1–20 像素 / 6 | 较小图片先用 3–6，大图先用 8–12。过大可能影响细小结构。 |
| Edge_Protection | 边缘保护 | 0–100 / 70 | 越高越保护明显明暗边缘，同时磨皮更温和。 |
| Texture | 纹理保留 | 0–100 / 60 | 越高越保留细小局部纹理。想更柔和时降低。 |
| Skin_Only | 仅肤色区域 | 开 / 开 | 建议保持开启；有精确皮肤选区时也可关闭。 |
| Skin_Range | 肤色识别范围 | 0–100 / 60 | 提高可覆盖更多偏冷、偏红的肤色，但也更容易匹配暖色背景。 |
| skinBrightening | 美白提亮（Skin Brightening） | 0–100 / 0 | 提亮皮肤中间调并轻微降低饱和度；0 不美白。可先试 20–30。 |
| skinRosiness | 红润（Skin Rosiness） | -100–100 / 0 | 正值增加红润，负值减轻泛红；0 不调节。可先试 10–20。 |
| Show_Mask | 显示作用遮罩 | 关 / 关 | 只检查范围时开启，正式磨皮时关闭。 |

**只调肤色、不磨皮**：将 `Strength` 设为 0，再调美白和红润。两者不受磨皮强度控制，沿用原图的肤色蒙版和 PhotoCraft 选区；磨皮不会改变调色的识别边界。美白和红润默认都为 0，保持旧版默认效果；三项强度均为 0 且关闭 `Show_Mask` 时，照片像素完全不变。显示蒙版时只显示原始作用范围，不叠加美白或红润。

可以从以下设置开始，随后根据原图调整：

- 自然人像：Strength 35–45，Radius 4–6，Edge_Protection 75，Texture 70。
- 更柔和：Strength 55–65，Radius 6–10，Edge_Protection 60，Texture 40。
- 暖色背景较多：先做皮肤选区，再用自然人像设置。

## 适用范围和限制

- 支持 **RGB 8/16/32 位**；非 RGB 模式会返回错误，不会按错误的颜色通道进行处理。先转 RGB，再选中照片像素图层。
- `Skin_Only` 是基于颜色的柔和筛选，不是人脸或皮肤语义识别。木头、沙地、暖色衣服、嘴唇也可能被选中；特殊灯光下皮肤也可能漏选。用 `Show_Mask` 检查，并配合选区更可靠。
- 主要作用是减弱皮肤细小起伏和噪点，不会识别并消除所有痘痘、斑点或皱纹。较大的瑕疵仍适合用修复画笔处理。
- 保留 Alpha；完全透明像素不参与邻域统计。RGB 中含超出 0–1 的 HDR 值、NaN 或无穷的像素保持不变，避免压坏高光。
- 美白和红润是像素级调色，不是自动统一肤色或语义识别；效果需要根据光线和肤色调整。
- 关闭 `Skin_Only` 后，选区内非肤色物体也会被柔化、提亮或调红。
- 磨皮较强时仍会损失一部分细节。建议在副本图层上操作，用不透明度控制最终效果。
- 1.0.1 的蒙版预览不再分配滤波缓存，效果处理合并成对的邻域计算，并跳过没有命中像素的图块。大图仍需要更多时间，详见[优化前后实测](performance-2026-11-07.md)；CLI 耗时不等同于 GUI 的实时预览响应时间。
- 智能滤镜重新计算时仍需要加载插件文件；本次未验证完整的智能对象往返流程。

## 实际处理语言

实际处理照片、生成肤色蒙版和计算滤波的代码位于 `src/lib.rs` 和 `src/regions.rs`，使用 **Rust** 编写，编译成 **WebAssembly（.wasm）** 后在 PhotoCraft 的本地沙箱中运行。PhotoCraft 0.20.2 使用 Rust 编写的 wasmi 解释器执行插件；本插件不调用 JavaScript 处理像素，也不上传照片。

仓库中的 JavaScript（`.mjs`）用于自动测试、性能测量、打包、校验和版本管理，不会随插件在用户电脑上执行。GitHub 显示的语言占比统计了这些开发工具，并不代表界面与算法的比例。插件没有自带 HTML/JavaScript 界面，参数面板由 PhotoCraft 主程序生成。

## 验证范围

- Rust 内核测试：滑动窗口与朴素计算对照、微小图片、透明度、无效/HDR 数据、肤色筛选、参数读取、无工作缓存的蒙版和不透明照片快速路径。
- WebAssembly 无外部导入，ABI 与清单可被真实 PhotoCraft 0.20.2 接受。
- 合成皮肤纹理测试中默认设置减弱局部噪点；蓝色区域保持不变。
- Texture 参数、零强度恒等、清晰边缘、Alpha/HDR/NaN 数据保留。
- Radius 1/6/20 的独立分块与整体计算对照，无超过阈值的边界差异。
- PhotoCraft 实际处理 1536×1024 图片，输出与不分块 WASM 参考结果的 8 位量化差异不超过 1 个色阶。
- PhotoCraft 的选区限制、Undo 恢复、RGB 16/32 位处理并保存。
- 用 scikit-image 收录的 NASA 公有领域 astronaut 样本完成一次默认设置处理和人工看图检查。此单张样本不能代表所有人像效果。

本地复现后，机器可读测试结果在 `test-output/verification.json`；首次本机集成记录见 [docs/verification-2026-11-06.json](verification-2026-11-06.json)。验证使用 CLI 引擎。

## 从源码编译（普通使用者可以跳过）

源码包没有第三方 Rust 依赖。需要 Rust 工具链和 wasm32 标准库；仓库中的 `rust-toolchain.toml` 会选择 stable Rust、目标库和开发组件。完整打包和 WASM 测试还需要 Node.js 24 或更新版本，无需 npm 安装依赖：

```sh
rustup target add wasm32-unknown-unknown
node scripts/version.mjs check
node --test tests/version.test.mjs
cargo test --locked --offline
cargo build --release --locked --offline --target wasm32-unknown-unknown
```

编译结果在 `target/wasm32-unknown-unknown/release/photocraft_natural_skin.wasm`。执行 `sh build.sh` 或 `./build.ps1` 还会生成带版本号的安装文件、清单、发布说明和校验和，保存到 `dist/`。

可选的 WebAssembly 与主程序集成测试需要 Node.js（没有 npm 依赖）：

```sh
node tests/verify.mjs
node tests/verify.mjs target/wasm32-unknown-unknown/release/photocraft_natural_skin.wasm /path/to/photocraft-cli
```

算法：带 Alpha 权重的标量引导滤波，叠加局部细节保护与平滑的肤色权重。所有处理均在本地 PhotoCraft 插件沙箱内运行。源码采用 MIT 许可证。

## 版本管理

首个版本为 **1.0.0**，发布日期和变更见 [CHANGELOG.md](../../../CHANGELOG.md)。版本采用 `主版本.次版本.修订版本`：兼容性修复升 patch，兼容的新功能升 minor，不兼容的插件 ID、参数或存档行为变化升 major；测试版可使用 `1.1.0-alpha.1`、`1.1.0-beta.1`、`1.1.0-rc.1`。

`Cargo.toml` 是权威版本来源。升版脚本会同步 `manifest.json`、`Cargo.lock` 和更新日志，不需要逐个手工修改：

```sh
node scripts/version.mjs                 # 查看版本
node scripts/version.mjs check           # 检查一致性
node scripts/version.mjs bump patch      # 示例：1.0.0 → 1.0.1
# 也可选择 minor、major，或者指定更高版本：
node scripts/version.mjs bump 1.1.0-beta.1
```

上面的升版命令是备选示例，不需要依次执行。补好当前版本的 `CHANGELOG.md` 后运行 `node scripts/version.mjs prepare`，将 `Unreleased` 替换为发布日期；也可以用 `prepare --date YYYY-MM-DD` 显式指定日期。脚本不会提交代码、创建标签或推送。

正式发布使用 `v<版本>` 标签。发布流程会核对版本、日期、更新说明和编译产物；预发布版会标记为 GitHub prerelease，不覆盖 Latest。已发布的标签不能移动，修复应发布新版本。详见[版本规则](versioning.md)和[发布流程](releasing.md)。

## 从 SoftSkin 迁移

这是原 SoftSkin 插件的正式更名版。当前菜单名为 **Natural Skin**，插件 ID 为 `io.github.wnanbei.photocraft.natural-skin`，与 Natural Skin 1.0.0 相同。如曾安装 SoftSkin，请从额外插件文件夹移走旧的 `SoftSkin-*.wasm`，避免同时出现两个菜单。旧版智能滤镜记录的插件 ID 不会自动替换，请保留旧插件直到相关文件迁移完成。
