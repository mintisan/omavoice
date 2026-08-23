# Handy integration

OmaVoice uses upstream [Handy](https://github.com/cjpais/handy) for local speech
recognition, the Wayland recording overlay, push-to-talk shortcut handling, and
text injection. Handy remains a separately configured component: OmaVoice does
not duplicate its model/API settings and does not vendor Handy source or build
artifacts.

The reviewed source revision and lock-file objects are fixed in
`build-pinned.sh`. Build dependencies must already be installed; the script
uses Bun 1.3.14 from mise, fetches locked npm/Cargo dependencies, and produces a
release build without downloading a speech-recognition model:

```bash
Linux/Handy/build-pinned.sh /tmp/omavoice-handy-build
```

Install the complete runtime without sudo. The private inference libraries and
Tauri resources are required alongside the binary:

```bash
Linux/Handy/install-user.sh --build-directory /tmp/omavoice-handy-build
```

The installer writes only these program files:

- `~/.local/bin/handy`
- `~/.local/bin/omavoice-handy` (routes only Handy to the stable ATVVoice source)
- `~/.config/systemd/user/omavoice-handy.service`
- `~/.local/lib/Handy/` (private inference libraries, resources, provenance)
- `~/.local/share/applications/com.pais.handy.desktop`
- `~/.local/share/icons/hicolor/256x256/apps/handy.png`
- `~/.local/share/licenses/Handy/LICENSE`

It deliberately preserves Handy's independently managed settings, recordings,
models, and cache. The OmaVoice launcher sets PipeWire's documented
`PIPEWIRE_NODE=atvvoice-omavoice-rc003` process environment, so Handy can use the
remote source without replacing the system-wide default microphone. In Handy's
own settings, select the `pipewire` microphone entry. The installer enables the
Handy user service by default; it follows `graphical-session.target`, starts
after the OmaVoice ATVVoice service, and does not run before a Wayland login.
Pass `--no-enable` only when the files should be installed without starting the
service. A staged install never calls systemd and can be verified without
changing the live user installation:

```bash
Linux/Handy/install-user.sh \
  --build-directory /tmp/omavoice-handy-build \
  --staging-root /tmp/omavoice-handy-root
```

Uninstall disables the user service and removes only the OmaVoice-managed program
files (again preserving settings and models):

```bash
Linux/Handy/install-user.sh --uninstall
```

Model downloads are separate, much larger, and have their own licenses. Do not
redistribute a model merely because Handy itself is MIT licensed.
