# 测试边界

[English](TESTING.md)

## 自动化门禁

- 锁定依赖的 Rust 测试、格式、Clippy 和 release 二进制。
- ATVVoice 补丁摘要、最终源码 tree、101 项测试和 release 构建。
- Handy 固定提交/tree/lock blob、运行时内容和逐文件摘要。
- Shell 语法、desktop 文件、systemd unit 和 PolicyKit XML。
- 隔离重复安装/卸载，保留用户数据和无关文件。
- 包路径/权限白名单、payload 摘要、x86_64 ELF 和缺失动态库检查。
- 英文默认和简体中文 locale 探针。
- 统计和文字档案只使用合成数据，不读取真实转写正文。

## 每个候选都需要的人工门禁

1. 在当前版本的干净 x86_64 Omarchy 上安装精确 Release 包。
2. 通过系统蓝牙界面配对 RC003，并验证稳定重连。
3. 长按 Mic：Handy 浮层出现、语音识别正确，松开后只向当前输入框写入一份完整结果。
4. 用实体遥控器验证 Power/Home、方向、OK、音量、菜单和自定义快捷键。
5. 分别在英文和中文 locale 下验证托盘/设置单实例以及全部侧边栏页面。
6. 明确开启文字档案，录制一句并验证数量/导出；关闭后再录一句，确认不再复制。
7. 验证统计增加，但不保存正文或按键序列。
8. 重启 PipeWire，并完成一次正常退出登录/重新登录；确认只有一个稳定输入源和四个健康用户服务。

## v0.1.0 尚不宣称

- RC001 兼容。
- 所有 Intel 蓝牙控制器都能自动恢复。
- 所有 GPU/Vulkan 驱动和语音模型。
- 多显示器悬浮位置和全部第三方应用。
- 未经实体验证即可突破遥控器固件的录音租期。

可使用 `sayallctl status`、`sayallctl doctor` 和四个用户 unit 的 `journalctl --user` 收集脱敏日志。不要公开 coredump、API Key、文字数据库、录音或未隐藏的蓝牙地址。
