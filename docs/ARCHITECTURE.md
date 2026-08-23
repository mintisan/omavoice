# OmaVoice architecture

[简体中文](ARCHITECTURE.zh-CN.md)

OmaVoice composes established open-source components behind narrow Linux integration boundaries.

```text
RC003 Bluetooth LE
        │ ATVV ADPCM
        ▼
patched ATVVoice ──────► stable PipeWire source
        │                         │
        │ D-Bus status            ▼
        │                       Handy
        │                  speech-to-text + overlay
        │                         │
        └──────────────┐          ▼
                       │      focused application
RC003 HID ─► keyd ─► keyboard actions
                │
                ├─► aggregate statistics (no key sequence)
                └─► F20 microphone hold trigger

OmaVoice settings/tray
  ├─ read-only doctor and runtime status
  ├─ user Profile and keyd preview
  ├─ optional PolicyKit helper for fixed keyd config
  ├─ private aggregate statistics
  └─ optional transcript archive (separate database, default off)
```

## Ownership boundaries

- **BlueZ** owns scanning, pairing, bonds and the Bluetooth controller.
- **Patched ATVVoice** owns ATVV protocol negotiation, ADPCM decoding and the PipeWire source.
- **Handy** owns recording, model/API configuration, speech recognition, overlay and text insertion.
- **keyd** owns low-level remote button remapping.
- **OmaVoice** owns orchestration, safe profiles, diagnostics, local aggregate statistics and the opt-in transcript copy.

OmaVoice does not add another Bluetooth stack, audio server, inference engine or arbitrary root command layer.

## Installed identifiers

Commands, units, desktop and PolicyKit IDs, the helper directory, keyd configuration and XDG paths use the `omavoice` namespace consistently.

## Privacy model

Aggregate statistics and transcript text use separate SQLite databases. Transcript capture is off by default. OmaVoice never copies audio or API keys into either database. Ordinary diagnostics redact device addresses and concrete input event nodes.

## Supply chain

ATVVoice is reconstructed from one pinned commit plus seven SHA-256-checked patches and a checked final Git tree. Handy is built from one pinned commit/tree with locked Cargo and Bun inputs. Release bundles include the exact build metadata and payload hashes.

Sanitized pairing, HID, audio-duration and controller-recovery evidence is kept in [RC003 hardware notes](RC003-HARDWARE.md).
