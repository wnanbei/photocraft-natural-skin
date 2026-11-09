# Natural Skin 0.2.0 桌面开发版

不修改 PhotoCraft，通过现有动作导出所选像素图层，使用独立的 Natural Skin 窗口预览，再将完整处理结果打开为 PhotoCraft 新文档。首版共用 Rust + egui 的 Windows/macOS 代码；两个平台分别编译。

## 第一次连接

1. 打开 Natural Skin，进入 **Connection**。选择 `PhotoCraft.app`（macOS）或 `PhotoCraft.exe`（Windows），点击 **Enable connection**。
2. 通过 **Show installer folder** 找到生成的 `Install Natural Skin.json`。在 PhotoCraft 中选择 **File → Scripts → Browse**（文件 → 脚本 → 浏览）导入。可使用 **Copy installer path** 复制路径。
3. 在 PhotoCraft 的动作面板找到 **Natural Skin Send**。若希望直接点击按钮，启用动作面板的“按钮模式”。
4. 选择一个普通像素图层并运行该动作。Natural Skin 接收快照，显示预览；调整参数后选择 **Apply to PhotoCraft**。

动作只需安装一次。重复导入同名动作会被 PhotoCraft 拒绝，已有动作不会被静默替换。需要重装时，只删除动作面板里的 **Natural Skin Send**，再导入脚本。

也可以跳过连接，直接通过 **Open Image** 打开支持的单层 `.pcraft` 或 RGB TIFF，再 **Export Result** 保存新文件。

## 平时使用与退出

设置保存在当前用户的数据目录，后续正常打开无需启动参数。重新打开会恢复此前启用的接收模式；再次运行程序会唤回已有实例。在导出期间收到的打开文件请求会等待导出结束。

连接启用时，关闭窗口会最小化并保持接收；宿主发送新图层时自动恢复。**Quit** 完全退出；写文件进入提交阶段后会等待提交完成。**Stop connection** 停止接收并记住停用状态。

**Start at login** 默认关闭，只有用户主动勾选才创建当前账户的启动项。macOS 使用自己的 LaunchAgent，Windows 使用当前用户的 Run 注册表项。请先将程序放到固定位置；移动程序后更新这个设置。此轮没有给开发机器的真实账户启用登录启动。

## 处理状态与结果找回

复制/校验快照、读取图像、准备预览、滤波、处理完整图像、写文件及交给 PhotoCraft 都有状态。执行中显示已用时间；后端提供完成量时显示当前阶段的百分比。没有可靠总量的阶段显示活动指示，不提供未经测量的剩余时间。接收校验和 OS 打开请求期间不提供取消；滤波取消需要等当前检查点，文件提交期间显示正在完成写入。

**Show skin mask** 仅影响预览，Apply/Export 始终输出正常效果。源图保持不变，结果发布到新路径，拒绝覆盖已有文件。回传结果是新文档，不会自动替换源图层。

结果和快照保留在磁盘上，没有自动清理策略。通过 **Connection → Open last result** 可以在 Natural Skin 重新打开上次结果，**Show saved files** 可定位存储目录。请将要保留的返回文档在 PhotoCraft 中另存，再自行管理文件。

默认数据目录：

- macOS：`~/Library/Application Support/Natural Skin`
- Windows：`%LOCALAPPDATA%\Natural Skin`

连接失败或 PhotoCraft 移动后，在 Connection 中选择正确应用并重新保存。OS 打开失败时，已完成的结果仍留在磁盘，不自动重复回传。配置格式损坏时显示错误，并保留原文件；需要用户在设置页重新保存连接。

## 本轮验收

在 macOS arm64 上通过隔离的 PhotoCraft 0.21.0 测试副本完成：首次连接、GUI 生成脚本、真实脚本导入、动作发送、最小化后再次接收、正常重启恢复设置、重复启动唤回同一实例、原生应用选择对话框、找回上次结果及蒙版开启时回传正常结果。宿主 Agent Control 关闭，没有控制端口；原安装和普通用户配置没有改动。

像素验收使用 64×48 RGBA16 合成图，包含透明像素下的 RGB 和 300 DPI。采集结果与测试源一致；GUI 输出与同参数 CLI 处理结果逐样本一致；原始测试文件哈希保持不变。该图没有 ICC，不能把此轮当作 ICC 或真人肤色效果验收。8/16/32 位、多图块及 ICC 的既有测试见 [原始交换报告](native-development-2026-11-08.zh-CN.md) 和 [动作路线报告](handoff-route-2026-11-09.zh-CN.md)。

设置失败会在固定底部状态栏显示原因和恢复方法，清除旧成功提示。已实际输入无效应用路径并确认错误可见、原有效连接保留。收尾评审对这条 P2 修复给出 **ship / resolved**，范围仅为 macOS 开发包的该项修复。

实际窗口检查覆盖正常尺寸和最小 880×620 客户区，参数及设置页均可滚动，工具栏与状态栏保持可见。自动测试、严格 Clippy、Windows 目标检查和本地打包结果记录在 [验收数据](desktop-0.2.json)。原生外观沿用 DESIGN.md；本轮是已有界面的功能扩展。已有 `.impeccable/design.json` 比 DESIGN.md 旧，本轮保留，后续可通过 document 同步。

## 发行边界

这是可运行的开发版。Windows 已通过目标检查并配置本机 CI 打包，尚未在 Windows 实机验证宿主唤起、启动项和界面；未生成或声称已验证 Windows 发行安装包。macOS 提供本地开发签名 `.app` 和 ZIP，未做正式签名/公证；远程 CI 尚未运行。

当前仅支持一个可见的普通 Normal 顶层像素图层，100% 不透明度和填充，无蒙版、剪贴、效果、选区、额外通道或时间线。完整处理上限 36 MP。显示按 sRGB 使用，ICC 数据保留不等于完成显示色彩管理；广色域/HDR、真实人像效果和大图峰值内存仍需进一步验收。
