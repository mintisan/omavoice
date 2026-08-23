# RC003 keyd mapping

This is the minimum reviewed mapping for the RC003 hardware evidence collected
on Omarchy Linux:

- match only the keyboard interface with stable vendor/product ID `2717:32b8`;
- map the verified microphone key from `f5` to Handy's reserved `f20` PTT key;
- leave every unspecified remote button and every other keyboard unchanged.

Structured `keyd monitor -t` capture has also verified these physical key
names with complete down/up edges:

```text
OK=enter  Up=up  Down=down  Left=left  Right=right
Back=back  Home=home  Menu=compose  TV=grave
Volume+=volumeup  Volume-=volumedown
```

The context-menu action deliberately renders `compose = compose`, not
`compose = menu`. On the current Omarchy XKB keymap, Linux `KEY_COMPOSE` is the
standard `Menu` keysym already proven by the physical remote, while keyd's
`menu` name emits the distinct `KEY_MENU` code that becomes `XF86MenuKB` and is
ignored by the tested application.

`Linux/SayAllLinux/src/keyd.rs` uses that table to generate the settings-page
preview and final system configuration through one keyd pipeline. It rejects
any profile other than the verified RC003 `2717:32b8`; volume buttons do not
require a separate hidraw daemon. Power remains deliberately absent from the
keyd keyboard mapping. The existing Mic-only configuration proves that this
omission preserves the remote's native Omarchy power menu, so
`Power = PassThrough` is safe to apply;
every custom Power action remains blocked until its original event and safe
suppression are verified. The checked-in file remains the minimum bootstrap
mapping; the settings page generates the reviewed ordinary-button mappings
from the selected device Profile.

Validate the file before installation:

```bash
keyd check Linux/keyd/sayall-rc003.conf
```

Installing keyd or writing `/etc/keyd` changes system-wide input handling and
requires explicit user confirmation. Install the fixed root-owned helper and
PolicyKit action separately from the user runtime:

```bash
bash Linux/install-keyd-helper.sh
```

The script builds the helper from this checkout and asks PolicyKit to install
only the fixed binary and policy. It does not get called implicitly by
`Linux/install-user.sh`. After installation, review the final preview in the
settings page and choose either **仅保存** (XDG Profile only) or **保存并应用**
(explicit PolicyKit authorization). The helper accepts only a bounded,
strictly validated RC003 configuration on stdin, runs `keyd check`, keeps a
root-only backup, atomically replaces the target, and reloads keyd. A failed
reload restores and reloads the previous configuration; applying identical
content does not rewrite or reload anything. Never apply a preview while it
reports an unsupported custom Power action or another unsupported action.

Handy and keyd both use an exclusive evdev grab. keyd must own the physical
RC003 before Handy starts so that Handy reads keyd's virtual F20 output. During
a first-time installation with Handy already running, exit Handy, start or
restart keyd, confirm the journal matches the physical device without
`Failed to grab`, then relaunch Handy through `sayall-handy`. A normal system
boot should start the system-level keyd service before the user session, but
that cold-login order still needs an end-to-end regression test.
