# OmaVoice 架构

[English](ARCHITECTURE.md)

OmaVoice 通过清晰、受限的 Linux 集成边界组合成熟的开源组件。

```text
RC003 蓝牙 LE
      │ ATVV ADPCM
      ▼
补丁版 ATVVoice ─────► 稳定 PipeWire 输入源
      │                        │
      │ D-Bus 状态             ▼
      │                      Handy
      │                 语音识别＋悬浮交互
      │                        │
      └─────────────┐          ▼
                    │       当前输入应用
RC003 HID ─► keyd ─► 键盘动作
                │
                ├─► 聚合统计（不保存按键序列）
                └─► F20 麦克风按住触发

OmaVoice 设置与托盘
  ├─ 只读能力诊断和运行状态
  ├─ 用户 Profile 与 keyd 预览
  ├─ 固定 keyd 配置的可选 PolicyKit helper
  ├─ 本机匿名聚合统计
  └─ 可选文字档案（独立数据库、默认关闭）
```

## 组件边界

- **BlueZ** 负责扫描、配对、Bond 和蓝牙控制器。
- **补丁版 ATVVoice** 负责 ATVV 协商、ADPCM 解码和 PipeWire 输入源。
- **Handy** 负责录音、模型/API 配置、语音识别、悬浮界面和文字上屏。
- **keyd** 负责底层遥控器按键映射。
- **OmaVoice** 负责组件编排、安全 Profile、诊断、本机聚合统计和用户主动开启的文字副本。

OmaVoice 不新增另一套蓝牙栈、音频服务器、推理引擎或任意 root 命令层。

## 兼容标识

首版有意保留 `sayall-*`、`app.sayall.*`、`/usr/lib/sayall`、`/etc/keyd/sayall-rc003.conf` 和 XDG `sayall/` 名称。这些标识连接了已验证的 unit、Profile、PolicyKit policy 和本机数据库。未来改名必须具备原子迁移和回滚方案。

## 隐私模型

匿名统计与文字正文使用独立 SQLite 数据库。正文采集默认关闭。OmaVoice 不把音频或 API Key 复制进数据库；普通诊断会隐藏设备地址和具体 input event 节点。

## 供应链

ATVVoice 由一个固定提交、七个 SHA-256 补丁和最终 Git tree 校验重建。Handy 固定提交/tree，并锁定 Cargo 与 Bun 输入。Release 包记录完整构建元数据和逐文件摘要。

配对、HID、音频时长和控制器恢复的脱敏证据见 [RC003 硬件记录](RC003-HARDWARE.zh-CN.md)。
