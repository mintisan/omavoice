# OmaVoice v0.1.0

Initial independent pre-release for current x86_64 Omarchy/Arch Linux.

Highlights:

- RC003 Bluetooth voice audio through a stable 16 kHz PipeWire source.
- Pinned Handy integration for global speech-to-text and overlay interaction.
- Configurable safe keyboard shortcuts through keyd and a fixed PolicyKit helper.
- Persistent OmaVoice settings/tray interface with English default and Simplified Chinese locale support.
- Private aggregate statistics and an optional, default-off local transcript archive.
- Source-pinned binary bundle with build metadata and SHA-256 verification.

Install dependencies on current x86_64 Omarchy/Arch:

```bash
sudo pacman -S --needed bluez bluez-utils pipewire pipewire-audio wireplumber gtk3 gtk4 libadwaita webkit2gtk-4.1 gtk-layer-shell libappindicator keyd polkit wtype wl-clipboard openblas vulkan-icd-loader zstd
sudo systemctl enable --now bluetooth.service keyd.service
```

Then download both release assets, verify `sha256sum --check SHA256SUMS`, extract the archive, and run `./install.sh`. See the repository README for RC003 pairing and required first-run Handy F20 setup.

This release does not bundle a speech-recognition model. It is a pre-release because RC001, broad GPU, extended sleep/reconnect, multi-display and full application-matrix validation remain open. See `docs/TESTING.md` in the source repository.

## 简体中文

这是面向当前 x86_64 Omarchy/Arch Linux 的首个独立预览版本。

主要内容：RC003 蓝牙语音转稳定 16 kHz PipeWire 输入源、固定 Handy 全局语音输入、keyd 安全自定义快捷键、OmaVoice 中英文设置/托盘、本机匿名统计，以及默认关闭的独立文字档案。

请先按上面的 `pacman` 命令安装依赖并启动蓝牙/keyd 服务，再同时下载两个 Release 文件、校验 SHA-256、解压并运行 `./install.sh`。RC003 配对与 Handy F20 首次设置步骤见仓库中文 README。

本版本不包含语音识别模型。由于 RC001、广泛 GPU、长期睡眠恢复、多显示器和完整应用矩阵仍待验证，因此保持 Pre-release。
