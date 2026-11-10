# 外观与独立语言选择

本轮将 Natural Skin 的独立窗口调整为 PhotoCraft Pro 系列的界面风格，并加入简体中文 / English 的即时切换和持久化。实现位于 `native/src/appearance.rs`、`native/src/i18n.rs`、`native/src/locales/zh-CN.tsv` 和 `native/src/native_menu.rs`。

## 外观来源与范围

参考本机 PhotoCraft 仓库的 `docs/ui-design.md`、`crates/ui-egui/src/theme.rs` 及其实际 macOS 窗口。检查时的本机提交为 `7698a0c104d040378de00d43ba6759aaabdfaaf1`。Dark / Gray / Light 分别接近 Pro / Pro Medium / Pro Light；另可跟随系统。字段、面板分隔、蓝色选中态与参数分组统一；图像四周保持中性深灰。Inter Regular 随插件嵌入，字体文件及 SIL OFL 授权位于 `native/assets/`，发行包保留第三方授权。

参数区改为标签、滑杆、可编辑数值同一行。工具栏与底部任务状态始终可见，右侧参数和连接页独立滚动。字号最低 11 pt；中文使用系统字形回退。

## 语言行为

- 顶部始终可见「语言」和「外观」，无需退出图像或中断处理。
- 首次按系统语言选择中文或英文；显式选择写入 Natural Skin 自己的设置，重启恢复。
- 控件、提示、连接说明、工作阶段、缓存/导出耗时、图像位深、色彩说明及已知错误均覆盖。切换时已显示的状态与错误也重新翻译。
- 应用提供的 macOS 菜单同步切换，保留编辑快捷键与原生响应链。
- 文件名、图层名、路径、CLI / 协议标识及已安装的 Natural Skin Send 动作名保持原样。
- 第三方或 OS 的未知错误显示所选语言的恢复提示，原始信息保留在「技术详情」。系统文件对话框按钮和 Services 内容按操作系统语言显示。
- 兼容没有新字段的旧版设置；设置保存失败有可见提示。损坏的设置不会被语言切换覆盖，连接错误也不会被保存语言偏好的成功状态清除。

## 验证

在 macOS arm64 上执行：

```sh
cargo fmt --all -- --check
cargo test --offline --locked --workspace --features gui
cargo clippy --offline --locked --workspace --all-targets --features gui -- -D warnings
cargo check --offline --locked --target x86_64-pc-windows-gnu --package natural-skin-companion --features gui --bins
sh build.sh
```

自动测试包含 30 项伴随程序测试、10 项 GUI 测试、1 项完整工作流测试和 24 项核心测试。新增回归覆盖目录键唯一与占位符一致、系统语言识别、旧设置迁移、所有外观与语言组合的持久化、重启恢复、损坏设置保护，以及 880×620 / 1120×760 下实际绘制文本的语言切换。切换过程中保留图像纹理、参数、就绪状态和错误；不发起额外滤波任务。

原生窗口观察使用隔离应用与 `test-output/interface-review/state` 配置，并加载既有的 240×180 RGBA16 合成图。已观察到中文深色和英文浅色图像工作区、Inter 字体与紧凑参数区、中英文连接页、重启恢复英文/浅色，以及无效 PhotoCraft 路径的中文错误和原连接保留。GUI 调试检查没有修改宿主偏好或原图。

这些结果不代表 Windows 原生窗口验收；本轮 Windows 验证仅为目标平台编译检查。现有 `.impeccable/design.json` 仍是旧预览索引；当前视觉规范以 `DESIGN.md` 和 Rust 实现为准。
