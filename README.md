# OmaVoice

[简体中文](README.zh-CN.md)

OmaVoice turns a compatible Bluetooth voice remote into a global dictation microphone and programmable input controller for Omarchy Linux.

> **Project status:** `v0.1.0` is an x86_64 Omarchy/Arch pre-release centered on the Xiaomi Bluetooth Voice Remote 2 Pro (RC003). It has substantial real-device validation on one Omarchy computer, but RC001 and universal Bluetooth/GPU compatibility are not yet claimed.

## Quick start

### 1. Install system dependencies

On a current Omarchy/Arch x86_64 installation:

```bash
sudo pacman -S --needed \
  bluez bluez-utils pipewire pipewire-audio wireplumber \
  gtk3 gtk4 libadwaita webkit2gtk-4.1 gtk-layer-shell libappindicator \
  keyd polkit wtype wl-clipboard openblas vulkan-icd-loader zstd
sudo systemctl enable --now bluetooth.service keyd.service
```

OmaVoice itself does not require an AUR package. A working vendor Vulkan driver is recommended; Handy also ships CPU inference backends.

### 2. Pair the RC003 remote

Use Omarchy's Bluetooth panel. Turn Bluetooth off on a previously paired Mac/TV first and unplug USB-C. Hold the remote's upper-right Live/TV button for about two seconds until the bottom indicator flashes, start **Add device**, select **Xiaomi Bluetooth Voice Remote** (it may initially appear as **MI RC**), then hold **Home + Menu** to complete pairing.

Pairing belongs to BlueZ and is intentionally not automated by OmaVoice. If the remote is not found, stop trying to connect from the old host and put the remote back into flashing pairing mode before rescanning.

See the [sanitized RC003 hardware notes](docs/RC003-HARDWARE.md) for verified button names, audio limits and recovery boundaries.

### 3. Download and install OmaVoice

Download the `.tar.zst` archive and `SHA256SUMS` from the same [GitHub Release](https://github.com/mintisan/omavoice/releases), then run:

```bash
sha256sum --check SHA256SUMS
tar --use-compress-program=unzstd -xf OmaVoice-v0.1.0-omarchy-arch-x86_64.tar.zst
cd OmaVoice-v0.1.0-omarchy-arch-x86_64
./install.sh
./optional-keyd/install.sh
```

`install.sh` is unprivileged and writes only to the current user's XDG and `~/.local` directories. The second command asks PolicyKit to authorize two explicit file installs: the fixed keyd helper and its narrow execution policy. It does not change `/etc/keyd` yet.

### 4. Complete first-run setup

1. Open **OmaVoice** from the app launcher.
2. On the button/profile page, review the RC003 defaults and choose **Save and apply**. This validates and atomically installs the fixed-device keyd mapping.
3. Open **Handy** from OmaVoice. Select the `pipewire` input, choose or download a speech model (or configure an API), keep push-to-talk enabled, and set Handy's **Transcribe** shortcut to **F20**. Pressing the remote Mic button while Handy is capturing the shortcut supplies F20.
4. Hold Mic, speak, and release. The Handy overlay should close and insert one completed result into the focused application.

OmaVoice never downloads a speech model or changes Handy's selected model/API. Models and hosted APIs have their own licenses and terms.

### 5. Verify the installation

```bash
sayallctl status
sayallctl doctor
```

The expected steady state is four active user services and one `atvvoice-sayall-rc003` PipeWire source. If Doctor reports missing evdev/uinput access, add the current user to the `input` group only after reviewing the request, then log out and back in.

## Update and uninstall

To update, verify and extract the newer release, then run its `./install.sh`. Repeated installation preserves settings, models, statistics and transcript data and refuses to overwrite locally modified managed files.

```bash
sayallctl uninstall
```

Uninstall removes only files whose hashes still match OmaVoice's install manifest. User configuration, Handy models/history, aggregate statistics and the optional transcript archive remain on disk. The two optional system keyd component files require the explicit administrator commands printed by `optional-keyd/uninstall.sh`; `/etc/keyd/sayall-rc003.conf` is deliberately retained for review.

## What it includes

- RC003 ATVV audio over Bluetooth LE to one stable 16 kHz mono PipeWire source.
- Pinned upstream [Handy](https://github.com/cjpais/handy) for local/API speech-to-text, overlay interaction and text insertion.
- keyd button mappings, safe configurable keyboard shortcuts and fixed F20 Mic push-to-talk.
- OmaVoice settings, persistent tray entry, diagnostics and service controls.
- Private aggregate usage statistics without transcript text or key sequences.
- Optional local transcript archive, disabled by default and never storing audio.

The first release keeps existing `sayall-*`, `app.sayall.*` and XDG `sayall/` identifiers as compatibility boundaries. They are implementation names, not the product brand.

## Language

English is the fallback and default. Simplified Chinese is selected when `LANGUAGE`, `LC_ALL`, `LC_MESSAGES` or `LANG` begins with `zh`.

```bash
LANG=en_US.UTF-8 sayall-settings
LANG=zh_CN.UTF-8 sayall-settings
```

## Privacy and privileges

- Bluetooth addresses are redacted from ordinary logs and diagnostics.
- Transcript text is copied only after explicit opt-in and is stored separately from anonymous statistics.
- OmaVoice statistics do not store audio, API keys, window titles, active application names or full key sequences.
- BlueZ owns scanning/pairing; PipeWire owns audio routing; Handy owns inference and model/API settings.
- Only the explicit optional PolicyKit flow can install or invoke the fixed `/usr/lib/sayall` helper.

## Build from source

Install the runtime dependencies from Quick start plus `base-devel`, `rust`,
`git`, `pkgconf`, `cmake`, `clang`, `shaderc`, and `vulkan-headers`. Install
[mise](https://mise.jdx.dev/) if it is not already present, then run:

```bash
cargo test --manifest-path Linux/SayAllLinux/Cargo.toml --locked --all-targets
cargo build --manifest-path Linux/SayAllLinux/Cargo.toml --locked --release --bins
bash Linux/ATVVoice/build-patched.sh /tmp/omavoice-atvvoice
mise install
bash Linux/Handy/build-pinned.sh /tmp/omavoice-handy
```

Third-party source revisions and lock files are pinned and checked; no mutable upstream source tree or speech model is vendored here. See [Architecture](docs/ARCHITECTURE.md), [testing boundaries](docs/TESTING.md) and [Contributing](CONTRIBUTING.md).

## Upstream and license

OmaVoice is an independent, unofficial Omarchy application. It is not endorsed by or affiliated with Omarchy, 37signals, Xiaomi, Handy, ATVVoice or the original SayAll project.

The Linux implementation derives from GPL-3.0-only [SayAll / remote-mic-app](https://github.com/HD838A/remote-mic-app) work. OmaVoice software is released under [GPL-3.0-only](LICENSE.md); third-party terms and the RC003 image notice are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
