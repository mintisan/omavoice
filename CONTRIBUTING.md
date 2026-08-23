# Contributing to OmaVoice

Thank you for helping improve OmaVoice.

## Development boundary

- Keep the repository Linux-only. Do not add macOS, iOS, Web, server or private planning material.
- Preserve the pinned ATVVoice and Handy source checks. Updating a pin requires source/tree verification and a focused review.
- Keep user-facing text in English and Simplified Chinese. English is the fallback language.
- Do not silently broaden privileged behavior. Pairing remains in BlueZ; system key mappings remain behind the fixed PolicyKit helper.
- Do not commit models, recordings, transcripts, statistics databases, logs, Bluetooth addresses, API keys or build directories.
- Keep installed identifiers in the `omavoice` namespace.

## Before opening a pull request

```bash
cargo fmt --manifest-path Linux/OmaVoiceLinux/Cargo.toml --all -- --check
cargo test --manifest-path Linux/OmaVoiceLinux/Cargo.toml --locked --all-targets
cargo clippy --manifest-path Linux/OmaVoiceLinux/Cargo.toml --locked --all-targets -- -D warnings
bash -n Linux/*.sh Linux/omavoicectl Linux/ATVVoice/*.sh Linux/Handy/*.sh Linux/Handy/omavoice-handy
python3 -m py_compile Linux/release/*.py
python3 Linux/release/check-markdown-links.py .
desktop-file-validate Linux/app.omavoice.Settings.desktop Linux/Handy/com.pais.handy.desktop
systemd-analyze --user verify Linux/systemd/*.service Linux/Handy/omavoice-handy.service
```

Hardware simulations and unit tests do not replace real Bluetooth, button, audio, overlay and text-insertion validation. State the exact manual boundary in every hardware-related change.
