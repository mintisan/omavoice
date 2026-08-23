# Security policy

OmaVoice is an early pre-release. Please avoid posting Bluetooth addresses, API keys, transcripts, recordings, database files, full diagnostics or coredumps in public issues.

For a security-sensitive report, use GitHub's private vulnerability reporting for this repository when available. Include the affected version, a minimal reproduction and sanitized logs.

The normal installer is unprivileged. The optional keyd flow accepts only a fixed, validated RC003 configuration through PolicyKit. A request to run a broader command as root is not part of OmaVoice and should be treated as suspicious.
