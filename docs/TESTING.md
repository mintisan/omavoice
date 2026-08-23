# Testing boundaries

[简体中文](TESTING.zh-CN.md)

## Automated gates

- Locked Rust tests, formatting, Clippy and release binaries.
- Pinned ATVVoice patch hashes, final source tree, 101 tests and release build.
- Pinned Handy commit/tree/lock blobs, runtime contents and payload hashes.
- Shell syntax, desktop entries, systemd units and PolicyKit XML.
- Isolated repeated install/uninstall with user data and unrelated files preserved.
- Package path/mode allowlist, payload hashes, x86_64 ELF inspection and unresolved-library checks.
- English-default and Simplified Chinese locale probes.
- Synthetic statistics and transcript history only; tests do not inspect real transcript text.

## Manual gates for every candidate

1. Install the exact release archive on a current clean Omarchy x86_64 system.
2. Pair RC003 through the system Bluetooth UI and verify stable reconnect.
3. Hold Mic: Handy overlay appears, speech is recognized, and releasing inserts one complete result into the focused application.
4. Verify Power/Home, direction, OK, volume, Menu and configured shortcut behavior with the physical remote.
5. Verify tray/settings single-instance behavior and every sidebar page in both English and Chinese locales.
6. Explicitly enable transcript capture, record one phrase, verify count/export, disable it, and verify a later phrase is not copied.
7. Verify statistics increment without transcript text or key sequences.
8. Restart PipeWire and perform a normal logout/login; verify one stable source and four healthy user services.

## Not yet claimed by v0.1.0

- RC001 compatibility.
- Universal Intel Bluetooth controller recovery.
- Every GPU/Vulkan driver and every speech model.
- Multi-display overlay placement and all third-party applications.
- Recording beyond the remote firmware's observed lease without physical validation.

Collect sanitized logs with `sayallctl status`, `sayallctl doctor`, and `journalctl --user` for the four managed units. Never publish a coredump, API key, transcript database, recording or unredacted Bluetooth address.
