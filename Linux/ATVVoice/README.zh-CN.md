# OmaVoice ATVVoice 补丁构建

[English](README.md)

这里保存 OmaVoice 在 Omarchy Linux 上使用的 ATVVoice 候选补丁链，不复制上游完整源码。上游项目采用 MIT 许可证：<https://github.com/b0o/ATVVoice>。

## 固定输入与输出

- 上游仓库：`https://github.com/b0o/ATVVoice.git`
- 上游基线：`f36286d8185cb2b9b219cd91a9c0e08091999c9d`
- 补丁后源码树：`df607e5c9609673fef683de1c02a3411b1acbd5d`
- 补丁完整性与顺序：由 `SHA256SUMS` 校验

七个补丁依次解决：新语音会话 ADPCM 状态重置、PipeWire 恢复时丢弃陈旧音频、设备事件流结束后的重连、GATT 重试前等待蓝牙重新连接、PipeWire 服务重启后的音源重建、RC001/RC003 九字节 legacy v1 capabilities 的 codec 布局，以及普通发现/占用日志泄露蓝牙地址。

前五项已在 RC003 小米蓝牙语音遥控器上完成对应真机验证。精确向量测试覆盖 legacy 解析器；标准九字节首包、首次协议协商和第一次 Mic 非零 PCM 已真机通过且没有重试，证明标准路径没有回归。解析器修复后尚未再次收到真实 legacy 首包。第七补丁已在同一真实设备被占用时复验：普通日志保留设备名称和占用状态，但不再包含地址。重建相同源码候选不能替代安装和端到端验收。

## 重建并验证

主机需要 Git、Rust/Cargo、`sha256sum`、`pkg-config`、PipeWire 开发库和 BlueZ/D-Bus 开发环境。在 Arch/Omarchy 上，对应基础组件通常为 `git`、`rust`、`pkgconf`、`pipewire` 和 `bluez`；脚本不会代为安装。

使用自动创建的临时目录：

```bash
bash Linux/ATVVoice/build-patched.sh
```

或指定一个不存在或为空的用户态输出目录：

```bash
bash Linux/ATVVoice/build-patched.sh /tmp/omavoice-atvvoice-check
```

脚本会依次校验补丁摘要、获取固定基线、应用补丁、校验最终 Git tree、执行 `cargo test --locked --all-targets` 并构建 release 二进制。它不会使用 `sudo`，也不会安装二进制、创建服务、修改 BlueZ/PipeWire 配置或启动 ATVVoice。输出目录会保留，便于审计构建结果。
