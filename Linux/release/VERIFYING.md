# Verifying an OmaVoice release

Run `sha256sum -c SHA256SUMS`, extract the sole archive, then from its root run
`sha256sum -c PAYLOAD-SHA256SUMS`. `BUILD-METADATA` records the pinned source
revisions and tool versions. The included `silero_vad_v4.onnx` performs voice
activity detection only; this release contains no speech-recognition model.
