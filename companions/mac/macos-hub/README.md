# macos-hub

MQTT bridge that exposes macOS audio control (Core Audio HAL default output
switching, system volume, CGEvent media keys) via the `service/macos/{edge_id}/...`
topic hierarchy — direct analog of `roon-hub`.

## Prerequisites

- macOS 14 (Sonoma) or newer, Apple Silicon or Intel
- Xcode Command Line Tools: `xcode-select --install`
- A reachable MQTT broker (mosquitto, EMQX, HiveMQ, etc.)
- Rust toolchain 1.75+ (stable): `rustup toolchain install stable`

## Build

```
cd companions/mac/macos-hub
cargo build --release
```

The resulting binary is `target/release/macos-hub`.

## Install

```
# Apple Silicon (Homebrew prefix /opt/homebrew):
sudo cp target/release/macos-hub /opt/homebrew/bin/

# Intel Mac:
sudo cp target/release/macos-hub /usr/local/bin/
```

Copy the config example:

```
mkdir -p "$HOME/Library/Application Support/macos-hub"
cp macos-hub.toml.example "$HOME/Library/Application Support/macos-hub/macos-hub.toml"
$EDITOR "$HOME/Library/Application Support/macos-hub/macos-hub.toml"
```

Edit `mqtt.host`, `mqtt.port`, `macos.edge_id`.

## LaunchAgent

1. Edit `launchd/com.shin1ohno.macos-hub.plist`: replace all `YOUR_USER`
   occurrences with your short username, and point `ProgramArguments[0]` at
   the binary location you chose above.
2. Create the log directory:
   ```
   mkdir -p "$HOME/Library/Logs/macos-hub"
   ```
3. Install and load:
   ```
   cp launchd/com.shin1ohno.macos-hub.plist "$HOME/Library/LaunchAgents/"
   launchctl load "$HOME/Library/LaunchAgents/com.shin1ohno.macos-hub.plist"
   ```
4. Verify:
   ```
   launchctl list | grep macos-hub
   tail -f "$HOME/Library/Logs/macos-hub/stderr.log"
   ```

## Permissions

CGEvent media-key posting requires either **Input Monitoring** or
**Accessibility** permission on recent macOS versions. Grant via:

System Settings → Privacy & Security → Accessibility → add the
`macos-hub` binary (click `+` and navigate to e.g. `/opt/homebrew/bin/macos-hub`).

If play/pause does not reach Music.app, add the binary under Input Monitoring
as well.

## Smoke test

In one terminal:

```
mosquitto_sub -h <broker-host> -t 'service/macos/#' -v
```

On startup you should see `service/macos/<edge_id>/state/volume`,
`.../state/output_device`, `.../state/available_outputs`, and
`.../state/playback_active` messages.

Send commands from another terminal:

```
# Media key
mosquitto_pub -h <broker-host> -t 'service/macos/<edge_id>/command/play_pause' -m ''

# Set absolute volume to 50%
mosquitto_pub -h <broker-host> -t 'service/macos/<edge_id>/command/volume' \
  -m '{"how":"absolute","value":50}'

# Step volume down one notch (-5%)
mosquitto_pub -h <broker-host> -t 'service/macos/<edge_id>/command/volume' \
  -m '{"how":"step","value":-1}'

# Switch default output (device_uid from available_outputs state topic)
mosquitto_pub -h <broker-host> -t 'service/macos/<edge_id>/command/set_output' \
  -m '{"device_uid":"BuiltInSpeakerDevice"}'

# Mute / unmute (approximated by volume=0 then restore)
mosquitto_pub -h <broker-host> -t 'service/macos/<edge_id>/command/mute' \
  -m '{"action":"toggle"}'
```

After each command, the subscriber terminal should show the corresponding
`state/volume` or `state/output_device` update (retained, QoS 1).

## Media keys troubleshooting

If media-key commands are accepted but Music.app does not react:

- Check Accessibility + Input Monitoring grants
- Some macOS versions require the **AppKit `NSEvent.otherEventWithType`**
  path rather than raw `CGEventCreate` + `CGEventSetType(14)`. See the
  comment at the top of `src/media_keys.rs` — if this shortcut path fails
  on your macOS version, the fix is to link AppKit and construct an
  NSEvent. This is flagged as an operator verification point.

## Uninstall

```
launchctl unload "$HOME/Library/LaunchAgents/com.shin1ohno.macos-hub.plist"
rm "$HOME/Library/LaunchAgents/com.shin1ohno.macos-hub.plist"
sudo rm /opt/homebrew/bin/macos-hub    # or /usr/local/bin/macos-hub
```

## Tests

Unit tests cover topic parsing and volume-command math — no FFI. Runs on
Linux too:

```
cargo test
```
