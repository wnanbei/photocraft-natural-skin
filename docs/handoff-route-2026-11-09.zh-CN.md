# 从 PhotoCraft 动作按钮唤起 Natural Skin：验证结果

结论：**这条路线在 macOS 上已经实际跑通，且不需要 PhotoCraft 的控制端口。** 原型已加入开发版，流程为“PhotoCraft 动作按钮 → 文件接收 → Natural Skin 窗口 → 应用 → 原 PhotoCraft 窗口的新文档”。这不是把原生界面嵌进 PhotoCraft。

## 实际验证

使用 PhotoCraft 0.21.0（efb7ebd0c）的隔离测试副本、专用配置与合成图；测试副本只改了 Bundle Identifier 和开发签名以隔离系统文件打开事件，机器指令段与安装版一致。没有修改已安装的 PhotoCraft 程序。测试配置始终是 `allowAgentControl: false`、`controlPort: 0`，没有传 `--control`。

| 环节 | 结果 |
| --- | --- |
| 从宿主发起 | 实际点击“动作”面板按钮，执行复制目标层到新文档、保存 `.pcraft` 副本、关闭临时文档三个步骤 |
| 动作安装 | 在已打开的 PhotoCraft 中经“文件 → 脚本 → 浏览”执行原生 JSON 安装脚本，创建并持久化了新动作，无需重启宿主 |
| 后台接收 | 接收器忽略启动前的旧文件；新请求生成独立快照并检查格式、ZIP 和图块内容，再载入预览 |
| 关窗后重入 | 关闭磨皮窗口后进程继续待命；再次点宿主按钮，同一后台进程恢复窗口并载入 `capture-2.pcraft` |
| 应用结果 | 点 “Apply to PhotoCraft”，全分辨率输出完成后通过系统文件打开机制送到原宿主窗口的新标签 |
| 蒙版查看 | 开启 Show Skin Mask 后应用，导出的仍是磨皮结果；与同参数完整内核结果比较为 0 误差 |
| 原图保持 | 源文件 SHA-256 未变；宿主中原文档另存验证副本，与输入逐样本比较为 0 误差 |

本轮样本为 64 × 48、RGBA16、变化 Alpha、300 DPI 的合成图。捕获、源图复查、原生应用输出的可见 RGB、完全透明像素下 RGB、Alpha 最大差异均为 **0**。本轮样本没有 ICC，不据此宣称验证了广色域；之前的高精度桌面往返测试另有实际 ICC 数据覆盖。

本地 **40 个测试**、GUI 全目标严格 Clippy 和 Windows GUI 目标编译检查通过。Windows 的真实按钮唤起/文件打开行为尚未实机验证。数据、产物哈希及证据路径见 [JSON](handoff-route-2026-11-09.json)。

## 发现并修正的问题

最初只在 `App::ui` 处理接收消息：egui/eframe 在窗口隐藏时会停止执行界面回调，导致文件虽已接收，窗口却不能自行回来。接收、任务完成和重绘调度现已移入 `App::logic`，另有“隐藏窗口处理请求”和“常驻模式关窗不退出”两个回归测试；修正后实际完成了关窗再唤起。

另一个限制是 Photoshop `.atn` 动作文件无法表示 `.pcraft` 保存步骤，直接导出会拒绝该步骤。原生 JSON 脚本可以通过 PhotoCraft 自己的命令创建动作，并保存到它的原生动作库。本原型采用后者，不把 `.atn` 当作安装载体。

## 开发版复现

先构建。交换目录应专用于 Natural Skin，接收器必须在点击动作前运行：

```sh
mkdir -m 700 -p /PRIVATE/EXCHANGE
./dist/natural-skin-companion handoff-action /PRIVATE/EXCHANGE /PRIVATE/Install-Natural-Skin.json
./dist/natural-skin --watch /PRIVATE/EXCHANGE --return-to /Applications/PhotoCraft.app
```

在 PhotoCraft 中运行一次“文件 → 脚本 → 浏览”，选择生成的 JSON。打开“窗口 → 动作”，切换“按钮模式”。以后选好目标像素图层，点 **Natural Skin Send** 即可显示伴随窗口；处理后点 **Apply to PhotoCraft**。

监听模式下，关窗口会隐藏并保持待命；窗口底部 **Stop Listening** 结束监听，之后可正常退出。普通不带 `--watch` 启动的窗口仍采用手动打开/导出流程。当前命令行参数是开发入口，还没有图形安装向导或登录自启动设置。

隔离验收环境可以这样准备和保持运行（macOS）：

```sh
python3 native/tests/probe_handoff.py
```

脚本只负责复制测试 app、创建测试配置/合成图、生成安装脚本并启动两个测试进程。屏幕上的按钮点击需实际完成；它不把脚本准备成功冒充为桌面测试通过。创建它打印的 `RUN/stop-route` 文件会关闭它拥有的测试进程。

## 当前边界

- Natural Skin 接收器必须已经在后台运行；单靠动作不能从完全退出状态启动外部原生程序。
- 只验证了当前支持的普通 RGB 像素层；蒙版、剪贴、图层效果等限制沿用原生适配器的范围。
- 返回新文档，不自动覆盖原图层。多文档打开时，关闭临时捕获文档可能改变宿主的活动标签；源标签焦点恢复尚未保证。
- 动作名称冲突、安装升级、托盘/菜单栏、开机待命和临时文件清理策略还没产品化。原生动作库的数据写入只是配置，不是安装宿主原生滤镜。
- Windows 路由需要实机验证；广色域/HDR 显示、真实照片质量、细粒度取消和 ETA 仍是后续工作。
