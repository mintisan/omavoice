# RC003 hardware notes

[简体中文](RC003-HARDWARE.zh-CN.md)

These notes preserve the sanitized hardware evidence collected with one Xiaomi
Bluetooth Voice Remote 2 Pro (RC003) on Omarchy Linux. They describe a verified
candidate, not every firmware revision or Bluetooth controller.

## Identity and transport

- The initial advertisement can be `MI RC`; after service discovery BlueZ can
  show `Xiaomi Bluetooth Voice Remote` or its localized equivalent.
- The verified HID keyboard interface is matched by keyd as `k:2717:32b8`.
  OmaVoice never stores a transient `/dev/input/eventN` path.
- One BLE connection carries two independent paths: HID over GATT becomes Linux
  evdev keys, while private ATVV GATT notifications become microphone audio
  through ATVVoice and PipeWire.
- On the tested unit, USB-C did not enumerate a USB HID or USB Audio device. It
  was useful for charging only and was unplugged before Bluetooth pairing.

## Pairing procedure that worked

1. Turn Bluetooth off on any previously paired Mac/TV, or remove/forget the
   remote there. One host continuing to reconnect can prevent discovery.
2. Unplug USB-C. Hold the remote's upper-right Live/TV button for about two
   seconds until the bottom white indicator starts flashing.
3. Start **Add device** in Omarchy's Bluetooth panel, select `MI RC` or
   `Xiaomi Bluetooth Voice Remote`, then hold **Home + Menu** to complete the
   bond. Keep this step even if one pairing attempt completes before it.
4. Wake the remote with **OK** if necessary. While it is awake, BlueZ should
   eventually report paired, bonded, trusted, connected, and services resolved.

The remote sleeps and reconnects on demand, so a desktop label alternating
between paired and connected is not by itself proof of failure. A usable HID
device, resolved services, ATVVoice negotiation, nonzero PCM, and the final text
result are stronger evidence. If an old bond is mismatched, remove only this
remote from BlueZ, put it back into flashing mode, and pair again.

## Verified buttons

Structured keyd monitoring captured complete down/up edges for:

| Physical button | Linux/keyd key |
| --- | --- |
| Mic | `f5` (reserved mapping to `f20`) |
| OK | `enter` |
| Up / Down / Left / Right | `up` / `down` / `left` / `right` |
| Back | `back` |
| Home | `home` |
| Menu | `compose` |
| TV | `grave` |
| Volume + / Volume - | `volumeup` / `volumedown` |

Volume initially appeared absent in one direct evdev observation while Omarchy's
volume OSD still responded. keyd's unified monitor later captured both complete
edges. The cause was the observation layer and an exclusive grab, not a special
volume protocol; OmaVoice therefore does not add a hidraw daemon. The Menu key
must remain `compose = compose`: mapping it to keyd's different `menu` code
produced an ignored `XF86MenuKB` event on the tested Omarchy keymap.

Power is deliberately passed through so Omarchy can show its native
power/restart/sleep dialog. The Mic mapping remains fixed to F20 push-to-talk;
it is not user-configurable.

## Verified audio contract

- ATVV v1.0 HoldToTalk, 16 kHz mono IMA/DVI ADPCM, 120-byte frames.
- Patched ATVVoice resets decoder state per session and publishes one stable
  `Audio/Source` named `atvvoice-sayall-rc003`.
- Handy is routed to that source with `PIPEWIRE_NODE` while the system default
  microphone remains unchanged.
- A real Mic hold produced the Handy bottom overlay, accurate Simplified
  Chinese recognition, and one completed insertion into the focused field.

On the tested firmware, a physical hold stopped at about 60 seconds even though
five `MIC_EXTEND` writes succeeded. Host-initiated `MIC_OPEN` started a control
session but produced only zero PCM; the following physical Mic session worked.
This supports an observed firmware/session lease, not a proven universal RC003
limit. OmaVoice v0.1.0 does not claim unlimited holds or an active-open bypass.

## Recovery boundary

The pinned ATVVoice patches cover stale queued audio, ended BlueZ event streams,
GATT retry while disconnected, PipeWire server restarts, session decoder reset,
legacy capability parsing, and address redaction. A PipeWire/WirePlumber restart
recreated one same-named source and the first following recording was audible.

If the Intel controller reports HCI transmit-completion timeouts and BlueZ can
no longer power the adapter on, restarting `bluetoothd` alone may not recover
the kernel driver/firmware. A system reboot recovered the tested controller and
preserved the bond. OmaVoice diagnoses this boundary but does not replace BlueZ,
reload kernel modules, or manage controller firmware.

See [Testing boundaries](TESTING.md) for what remains unclaimed by v0.1.0.
