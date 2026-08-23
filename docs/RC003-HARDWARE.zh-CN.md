# RC003 硬件记录

[English](RC003-HARDWARE.md)

本文保存一只小米蓝牙语音遥控器 2 Pro（RC003）在 Omarchy Linux 上的脱敏真机证据。它描述的是已验证候选，不代表所有固件版本和蓝牙控制器。

## 身份与传输

- 初始广播名可能是 `MI RC`；完成服务解析后，BlueZ 可能显示 `Xiaomi Bluetooth Voice Remote` 或对应本地化名称。
- 已验证的 HID 键盘接口由 keyd 以 `k:2717:32b8` 匹配。OmaVoice 不保存会变化的 `/dev/input/eventN` 路径。
- 同一条 BLE 连接承载两条独立路径：HID over GATT 变成 Linux evdev 按键；私有 ATVV GATT 通知经 ATVVoice 和 PipeWire 变成麦克风音频。
- 实测设备通过 USB-C 接入时没有枚举 USB HID 或 USB Audio，只能确认充电作用；蓝牙配对前已拔线。

## 已成功的配对步骤

1. 关闭此前配对 Mac/电视的蓝牙，或在旧主机上忽略该遥控器。旧主机持续抢连可能阻止发现。
2. 拔掉 USB-C，长按遥控器右上角直播/TV 键约 2 秒，直到底部白灯开始闪烁。
3. 在 Omarchy 蓝牙面板开始**添加设备**，选择 `MI RC` 或**小米蓝牙语音遥控器**，再按住**主页键 + 菜单键**完成 bond。即使某一次样本没有补按就成功，也应保留此步骤。
4. 必要时按 **OK** 唤醒遥控器。遥控器清醒时，BlueZ 最终应同时具备 paired、bonded、trusted、connected 和 services resolved。

遥控器会休眠并按需重连，因此桌面状态在 paired 与 connected 之间变化本身不等于失败。可用 HID、服务解析、ATVVoice 协商、非零 PCM 和最终文字才是更强证据。如果旧 bond 失配，只删除 BlueZ 中这只遥控器，让它重新闪烁后再配对。

## 已验证按键

keyd 结构化监视捕获了以下完整 down/up 边沿：

| 实体按键 | Linux/keyd 键名 |
| --- | --- |
| Mic | `f5`（固定映射到 `f20`） |
| OK | `enter` |
| 上 / 下 / 左 / 右 | `up` / `down` / `left` / `right` |
| 返回 | `back` |
| 主页 | `home` |
| 菜单 | `compose` |
| TV | `grave` |
| 音量+ / 音量− | `volumeup` / `volumedown` |

最初直接观察一个 evdev 节点时没有看到音量事件，但 Omarchy 音量 OSD 正常响应；随后 keyd 统一监视器捕获了两键完整边沿。原因是观察层级和独占 grab，不是独立音量协议，因此 OmaVoice 不增加 hidraw daemon。菜单键必须保留 `compose = compose`；把它改成 keyd 的另一个 `menu` code 后，当前 Omarchy 键盘映射产生无人处理的 `XF86MenuKB`。

Power 有意保持原键，由 Omarchy 显示原生关机/重启/睡眠界面。Mic 固定映射为 F20 Push-to-Talk，不允许用户自定义。

## 已验证音频契约

- ATVV v1.0 HoldToTalk，16 kHz 单声道 IMA/DVI ADPCM，120-byte frame。
- 补丁版 ATVVoice 每个会话重置 decoder，并发布唯一稳定 `Audio/Source`：`atvvoice-sayall-rc003`。
- 使用 `PIPEWIRE_NODE` 只把 Handy 路由到该 source，不改变系统默认麦克风。
- 实体 Mic 长按已触发 Handy 底部浮层、准确简体中文识别，并向当前输入框完成一次完整写入。

实测固件中，物理长按约 60 秒后停止，即使五次 `MIC_EXTEND` 写入均成功。主机主动 `MIC_OPEN` 能启动控制会话，但只产生全零 PCM；紧接着的物理 Mic 会话正常。这支持“当前固件/会话存在已观察租期”，不能证明所有 RC003 都有统一硬限制。OmaVoice v0.1.0 不宣称无限长按或主动开麦绕过。

## 恢复边界

固定 ATVVoice 补丁覆盖陈旧音频队列、BlueZ 事件流结束、断线时 GATT 重试、PipeWire server 重启、会话 decoder 重置、legacy capability 解析和地址脱敏。PipeWire/WirePlumber 重启后已重建唯一同名 source，且随后第一次录音可听清。

如果 Intel 控制器持续报告 HCI transmit-completion timeout，且 BlueZ 已无法重新打开 adapter，只重启 `bluetoothd` 可能无法恢复内核驱动/固件。实测系统重启后控制器恢复且 bond 保留。OmaVoice 只诊断这条边界，不替换 BlueZ、不重载内核模块，也不管理控制器固件。

v0.1.0 尚未宣称的范围见[测试边界](TESTING.zh-CN.md)。
