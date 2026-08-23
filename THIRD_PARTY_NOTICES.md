# Third-party notices

## SayAll / remote-mic-app

- Source: <https://github.com/HD838A/remote-mic-app>
- Split basis: `1d7ad8da3dfa147403b2b24f12ac8c09c16c0d08`
- License: GPL-3.0-only

OmaVoice derives its Linux settings, diagnostics, statistics, transcript archive, install lifecycle and integration work from this project. The original proprietary SayAll App Logo is not included.

## remote-bridge-hub

- Source: <https://github.com/xxb26553663-star/remote-bridge-hub>
- Reference revision: `8a93f321ac71a602300c6cd77f7256fa4b63068e`
- License: GPL-3.0-only

RC003 ATVV UUIDs, microphone commands, ADPCM decoding order, capability parsing and HID usage mapping were informed by this project.

## ATVVoice

- Source: <https://github.com/b0o/ATVVoice>
- Base commit: `f36286d8185cb2b9b219cd91a9c0e08091999c9d`
- Reviewed patched tree: `df607e5c9609673fef683de1c02a3411b1acbd5d`
- License: MIT

OmaVoice applies the seven reviewed, SHA-256-pinned patches under `Linux/ATVVoice/patches/`. The full MIT text is in `LICENSES/ATVVoice-MIT.txt` and is included in binary releases.

## Handy

- Source: <https://github.com/cjpais/handy>
- Commit: `9bcb6d9d46c88517d2b5519d3a4f900ee3968c99`
- Tree: `65254d74f1a0465ac684790f29a79c9c894c5dc1`
- License: MIT

Handy remains an independently configured upstream application. OmaVoice does not claim ownership of Handy, its inference libraries, or its model/API integrations. The full MIT text is in `LICENSES/Handy-MIT.txt` and is included in binary releases.

## Silero VAD

Handy's pinned runtime contains `silero_vad_v4.onnx`, used only for voice activity detection. Silero VAD is published at <https://github.com/snakers4/silero-vad> under the MIT license. OmaVoice does not bundle a speech-recognition model.
The model file in the pinned Handy tree has SHA-256 `a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28`; the Silero MIT text is included as `LICENSES/Silero-VAD-MIT.txt`.

## Rust crates and Handy JavaScript packages

Dependency names, exact versions, declared license expressions and all license/notice files present in the locked build inputs are collected into every binary release under `LICENSES/Rust/` and `LICENSES/JavaScript/`. This covers the OmaVoice, ATVVoice and Handy Rust graphs, Handy's installed Bun dependency tree, and the vendored transcribe.cpp/ggml notices shipped inside its Rust crates. An empty `license_files` field in an index means the published package declared a license but did not include a text file. System dynamic libraries are installed by Arch packages and are not copied into the release.

## RC003 product photograph

`Resources/RC003-remote-photo.png` was supplied by the user for the physical-button mapping interface. Copyright and trademark rights in the photograph and depicted Xiaomi product remain with their respective owners. The GPL-3.0-only software license does not grant additional rights to this image or the Xiaomi marks.

## Speech models and APIs

Speech-recognition models and hosted APIs have their own licenses and terms. OmaVoice neither downloads nor redistributes a model. Users select and obtain models or API access through Handy.
