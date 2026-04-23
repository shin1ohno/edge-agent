# macos-hub

MQTT bridge for macOS audio control: output-device switching (Core Audio HAL),
system volume, and media-key injection (CGEvent). Direct analog of
`roon-hub` — same `service/{type}/{target}/state|command/{name}` topic
structure, same `MqttBridge` shape.

Runs as a LaunchAgent in the user's Aqua session (required for CGEvent
media-key injection to reach AppKit).

## Prerequisites

- macOS 14+ (Sonoma or later)
- Rust toolchain (`rustup` + `cargo`)
- An MQTT broker reachable from this Mac (e.g. `mosquitto` on the LAN)
- `mosquitto_pub` / `mosquitto_sub` for smoke testing (`brew install mosquitto`)

## Build

```
cd companions/mac/macos-hub
cargo build --release
```

The binary lands at `companions/mac/macos-hub/target/release/macos-hub`.
Install it:

```
sudo cp target/release/macos-hub /usr/local/bin/macos-hub
```

## Configure

```
mkdir -p ~/.config/macos-hub
cp macos-hub.toml.example ~/.config/macos-hub/macos-hub.toml
$EDITOR ~/.config/macos-hub/macos-hub.toml
```

Set `mqtt.host` and a stable `macos.edge_id` (e.g. `"air"`, `"studio"`).

## Install launchd agent

1. Edit `launchd/com.shin1ohno.macos-hub.plist` — replace `REPLACE_ME`
   with your macOS username in the config path.
2. Copy and load:
   ```
   cp launchd/com.shin1ohno.macos-hub.plist ~/Library/LaunchAgents/
   launchctl load ~/Library/LaunchAgents/com.shin1ohno.macos-hub.plist
   ```
3. Tail logs:
   ```
   tail -f /tmp/macos-hub.err.log
   ```

To stop / reload:
```
launchctl unload ~/Library/LaunchAgents/com.shin1ohno.macos-hub.plist
launchctl load   ~/Library/LaunchAgents/com.shin1ohno.macos-hub.plist
```

## Smoke test

In one terminal, watch all macos-hub state:
```
mosquitto_sub -h <broker> -t 'service/macos/#' -v
```

In another, drive commands against `edge_id = "air"`:
```
# Play / pause
mosquitto_pub -h <broker> -t 'service/macos/air/command/play_pause' -m ''

# Next / previous
mosquitto_pub -h <broker> -t 'service/macos/air/command/next' -m ''
mosquitto_pub -h <broker> -t 'service/macos/air/command/previous' -m ''

# Absolute volume
mosquitto_pub -h <broker> -t 'service/macos/air/command/volume' \
  -m '{"how":"absolute","value":50}'

# Relative +10
mosquitto_pub -h <broker> -t 'service/macos/air/command/volume' \
  -m '{"how":"relative","value":10}'

# Step ±5
mosquitto_pub -h <broker> -t 'service/macos/air/command/volume' \
  -m '{"how":"step","value":1}'

# Switch output (paste a UID you see in state/available_outputs)
mosquitto_pub -h <broker> -t 'service/macos/air/command/set_output' \
  -m '{"device_uid":"AppleAirPlay-xxxxxxx"}'
```

## Permissions (one-time)

macOS will prompt for **Input Monitoring** / **Accessibility** the first
time CGEvent injection is attempted. Grant it to the `macos-hub` binary
in *System Settings → Privacy & Security*.

Audio control via Core Audio HAL does not require additional permissions
on consumer Macs.

## Known TODOs

- `playback_active` is always published as `null`. Accurate detection
  needs per-process audio-level sampling, which is out of scope.
- `mute` approximates by setting volume to 0 / restoring to 50%. The
  proper path is `kAudioDevicePropertyMute` per output channel — not
  yet implemented.
- The CGEvent media-key path uses `kCGEventSourceUserData (42)` to
  pack `data1`. If certain apps do not receive the events, the fallback
  is `NSEvent.otherEventWithType:...subtype:8:data1:...` via `objc2`.
