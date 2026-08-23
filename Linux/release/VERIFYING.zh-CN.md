# 验证 OmaVoice 发布包

先运行 `sha256sum -c SHA256SUMS`，解压唯一的归档，再在其根目录运行
`sha256sum -c PAYLOAD-SHA256SUMS`。`BUILD-METADATA` 记录固定的源码版本和工具版本。
包内的 `silero_vad_v4.onnx` 仅用于语音活动检测；本发布包不含语音识别模型。
