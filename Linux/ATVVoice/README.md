# Building the OmaVoice ATVVoice patch set

[简体中文](README.zh-CN.md)

This directory stores the reviewed ATVVoice patch chain used by OmaVoice on
Omarchy Linux without copying the complete upstream source. Upstream is MIT
licensed: <https://github.com/b0o/ATVVoice>.

## Pinned inputs and output

- Upstream repository: `https://github.com/b0o/ATVVoice.git`
- Upstream base: `f36286d8185cb2b9b219cd91a9c0e08091999c9d`
- Patched source tree: `df607e5c9609673fef683de1c02a3411b1acbd5d`
- Patch integrity and order: `SHA256SUMS`

The seven patches reset ADPCM state for each voice session, discard stale
audio when PipeWire resumes, reconnect after the device event stream ends,
wait for Bluetooth reconnection before retrying GATT, recreate the source after
a PipeWire server restart, parse the RC001/RC003 nine-byte legacy v1 capability
layout, and redact Bluetooth addresses from ordinary discovery/busy logs.

The first five fixes have corresponding RC003 hardware validation. Exact-vector
tests cover the legacy parser, while one real standard nine-byte handshake
proved the normal path still negotiates and produces nonzero PCM without a
retry. A real legacy packet has not yet reappeared after the parser fix. The
redaction patch was checked while the same physical device was busy: ordinary
logs retained its name and busy state without exposing its address. Rebuilding
this source candidate does not replace installation or end-to-end validation.

## Rebuild and verify

The host needs Git, Rust/Cargo, `sha256sum`, `pkg-config`, PipeWire development
files, and the BlueZ/D-Bus build environment. On Arch/Omarchy the core packages
are normally `git`, `rust`, `pkgconf`, `pipewire`, and `bluez`; the script does
not install them.

Use an automatically created temporary directory:

```bash
bash Linux/ATVVoice/build-patched.sh
```

Or provide a nonexistent or empty user-owned output directory:

```bash
bash Linux/ATVVoice/build-patched.sh /tmp/omavoice-atvvoice-check
```

The script verifies patch hashes, fetches the pinned base, applies the patches
in order, verifies the final Git tree, runs `cargo test --locked --all-targets`,
and builds the release binary. It does not use `sudo`, install files, create
services, change BlueZ/PipeWire configuration, or start ATVVoice. The output
directory remains available for audit.
