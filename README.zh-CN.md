# OmaVoice

[English](README.md)

OmaVoice 可以把兼容的蓝牙语音遥控器变成 Omarchy Linux 上的全局语音输入麦克风和可配置按键控制器。

> **项目状态：** `v0.1.0` 是面向 x86_64 Omarchy/Arch 的预览版本，主要支持小米蓝牙语音遥控器 2 Pro（RC003）。它已在一台 Omarchy 电脑上完成较充分的真机测试，但尚不宣称支持 RC001 和所有蓝牙/GPU 环境。

## 快速开始

### 1. 安装系统依赖

在当前版本的 x86_64 Omarchy/Arch 上运行：

```bash
sudo pacman -S --needed \
  bluez bluez-utils pipewire pipewire-audio wireplumber \
  gtk3 gtk4 libadwaita webkit2gtk-4.1 gtk-layer-shell libappindicator \
  keyd polkit wtype wl-clipboard openblas vulkan-icd-loader zstd
sudo systemctl enable --now bluetooth.service keyd.service
```

OmaVoice 本身不需要 AUR 包。建议使用可工作的厂商 Vulkan 驱动；Handy 同时包含 CPU 推理后端。

### 2. 配对 RC003 遥控器

使用 Omarchy 的蓝牙面板。先关闭之前已配对 Mac/电视的蓝牙并拔掉 USB-C。长按遥控器右上角直播/TV 键约两秒，直到底部指示灯闪烁；点击**添加设备**，选择**小米蓝牙语音遥控器**（首次扫描也可能显示 **MI RC**），再按住**主页键 + 菜单键**完成配对。

配对由 BlueZ 负责，OmaVoice 不会擅自接管。如果扫描不到，请先停止旧设备的连接尝试，让遥控器重新进入闪烁配对状态后再扫描。

已验证键名、音频时长和恢复边界见[脱敏 RC003 硬件记录](docs/RC003-HARDWARE.zh-CN.md)。

### 3. 下载并安装 OmaVoice

从同一个 [GitHub Release](https://github.com/mintisan/omavoice/releases) 下载 `.tar.zst` 和 `SHA256SUMS`，然后运行：

```bash
sha256sum --check SHA256SUMS
tar --use-compress-program=unzstd -xf OmaVoice-v0.1.0-omarchy-arch-x86_64.tar.zst
cd OmaVoice-v0.1.0-omarchy-arch-x86_64
./install.sh
./optional-keyd/install.sh
```

`install.sh` 不使用 root，只写当前用户的 XDG 和 `~/.local` 目录。第二条命令请求 PolicyKit 明确授权两次文件安装：固定 keyd helper 及其受限执行 policy；此时还不会修改 `/etc/keyd`。

### 4. 完成首次设置

1. 从应用启动器打开 **OmaVoice**。
2. 在按键/Profile 页面检查 RC003 默认映射，然后选择**保存并应用**。程序会校验并原子安装仅适用于固定设备的 keyd 映射。
3. 从 OmaVoice 打开 **Handy**。选择 `pipewire` 输入，选择/下载语音模型（或配置 API），保持按住说话模式开启，并把 Handy 的 **Transcribe** 快捷键设为 **F20**。Handy 捕获快捷键时按遥控器 Mic 键即可输入 F20。
4. 长按 Mic 说话后松开；Handy 悬浮层应结束，并向当前输入框写入一份完整结果。

OmaVoice 不会下载语音模型，也不会改变 Handy 已选择的模型/API。模型和在线 API 有各自的许可证与条款。

### 5. 验证安装

```bash
sayallctl status
sayallctl doctor
```

正常状态是四个用户服务持续运行，并且只有一个 `atvvoice-sayall-rc003` PipeWire 输入源。如果 Doctor 提示缺少 evdev/uinput 权限，请先核对原因，再把当前用户加入 `input` 组，并重新登录。

## 更新与卸载

更新时，校验并解压新 Release，再运行其中的 `./install.sh`。重复安装会保留设置、模型、统计和文字档案，并拒绝覆盖已被本地修改的托管文件。

```bash
sayallctl uninstall
```

卸载只删除摘要仍与 OmaVoice 安装清单一致的文件；用户配置、Handy 模型/历史、匿名统计和可选文字档案都会保留。两个可选的系统 keyd 组件需要按 `optional-keyd/uninstall.sh` 打印的管理员命令显式删除；`/etc/keyd/sayall-rc003.conf` 会保留，供用户核对。

## 包含的功能

- 通过蓝牙 LE 接收 RC003 ATVV 语音，提供唯一稳定的 16 kHz 单声道 PipeWire 输入源。
- 固定上游 [Handy](https://github.com/cjpais/handy)，完成本地/API 语音识别、悬浮交互和文字上屏。
- keyd 按键映射、安全的自定义键盘快捷键，以及固定 F20 的 Mic 按住说话。
- OmaVoice 设置页、常驻托盘、诊断和服务控制。
- 不保存正文或按键序列的本机匿名聚合统计。
- 默认关闭的本机文字档案；始终不保存音频。

首版继续保留 `sayall-*`、`app.sayall.*` 和 XDG `sayall/` 内部标识作为兼容边界；这些不是产品品牌。

## 语言

英文是默认和兜底语言。当 `LANGUAGE`、`LC_ALL`、`LC_MESSAGES` 或 `LANG` 以 `zh` 开头时使用简体中文。

```bash
LANG=en_US.UTF-8 sayall-settings
LANG=zh_CN.UTF-8 sayall-settings
```

## 隐私与权限

- 普通日志和诊断会隐藏蓝牙地址。
- 只有用户明确开启后才复制文字正文，并与匿名统计分库保存。
- OmaVoice 统计不保存音频、API Key、窗口标题、当前应用名称或完整按键序列。
- BlueZ 负责扫描/配对，PipeWire 负责音频路由，Handy 负责推理和模型/API 设置。
- 只有显式的可选 PolicyKit 流程才能安装或调用固定的 `/usr/lib/sayall` helper。

## 从源码构建

除了“快速开始”的运行依赖，还需安装 `base-devel`、`rust`、`git`、
`pkgconf`、`cmake`、`clang`、`shaderc` 和 `vulkan-headers`。如果系统尚无
[mise](https://mise.jdx.dev/)，请先安装，然后运行：

```bash
cargo test --manifest-path Linux/SayAllLinux/Cargo.toml --locked --all-targets
cargo build --manifest-path Linux/SayAllLinux/Cargo.toml --locked --release --bins
bash Linux/ATVVoice/build-patched.sh /tmp/omavoice-atvvoice
mise install
bash Linux/Handy/build-pinned.sh /tmp/omavoice-handy
```

第三方源码版本与锁文件均固定校验；仓库不复制可变的完整上游源码，也不包含语音模型。更多信息见[架构说明](docs/ARCHITECTURE.zh-CN.md)、[测试边界](docs/TESTING.zh-CN.md)和[贡献指南](CONTRIBUTING.md)。

## 上游与许可证

OmaVoice 是独立、非官方的 Omarchy 应用，与 Omarchy、37signals、小米、Handy、ATVVoice 和原 SayAll 项目不存在官方授权、赞助或隶属关系。

Linux 实现派生自 GPL-3.0-only 的 [SayAll / remote-mic-app](https://github.com/HD838A/remote-mic-app)。OmaVoice 软件按 [GPL-3.0-only](LICENSE.md) 发布；第三方条款和 RC003 图片声明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
