# edge-agent

Device-host agent that bridges physical IoT controllers (Nuimo, Hue Tap Dial) to home-audio / lighting services (Roon, Philips Hue, macOS audio, Apple Music on iPad). Each edge host runs a single `edge-agent` binary that talks directly to the services it controls and syncs config + state with [`weave-server`](https://github.com/shin1ohno/weave) over a single WebSocket — no MQTT broker required.

## Why this exists

At home scale — one Roon Core, a handful of devices, each Nuimo physically near one host — putting an MQTT broker in the middle is overkill. This agent removes two hops:

- **Before**: `nuimo-mqtt` → MQTT broker → `weave-engine` → MQTT broker → `roon-hub` → Roon Core.
- **After**: `edge-agent` (one binary, one host) → Roon Core / Hue Bridge / macOS audio / iPad media stack.

The MQTT path (via `roon-hub` + `nuimo-mqtt` + `weave-engine`) is still maintained for N:N cross-host topologies. See "Which path?" below.

## Architecture

```
[edge-agent] × N  (per device host)
 ├ input devices    Nuimo (BLE, multi-device per host)
 │                  Hue Tap Dial Switch (forwarded over Hue v2 SSE)
 │                  iPad keyboards / Apple Music remote (companion iOS app)
 ├ routing engine   input primitive → service intent (config-driven, pure)
 ├ service adapters adapter_roon  — direct Roon API
 │                  adapter_hue   — direct CLIP v2 + bridge SSE
 │                  adapter_macos — MQTT bridge to macos-hub on the Mac
 │                  adapter_ios_media — MQTT bridge to the iPad app
 └ ws client        /ws/edge: config pull (ConfigFull/Patch, GlyphsUpdate) +
                    state push (StateUpdate, DeviceState[input,connected,battery])

                          │ WebSocket, LAN, no auth
                          ▼

[weave-server]  config + state hub (SQLite + axum + /ws/edge + /ws/ui + Next.js Web UI)
```

Each edge-agent registers as its own Roon Extension (unique `extension_id`) so multiple edges coexist cleanly under Roon's account model.

### macOS bridge (`companions/mac/macos-hub`)

A standalone Rust binary that runs on the Mac, owns Core Audio + MediaRemote + osascript, and exposes them on MQTT (`service/macos/<edge_id>/{state,command}/<property>`). edge-agent's `adapter_macos` is the MQTT consumer side — the Mac is the speaker host, but the routing engine and Nuimo BLE stay on whichever Linux host is closest to the user. Built and packaged via the `macos-hub` mitamae cookbook (in the [`setup`](https://github.com/shin1ohno/setup) repo) into a `~/Applications/MacosHub.app` bundle so macOS Local Network Privacy can grant it persistent LAN access.

### iOS companion (`ios/`)

A native SwiftUI app that publishes the same `device_state[input]` frames a Linux edge does, so an iPad becomes a portable edge:

- Apple Music control via `MPMusicPlayerController` (iOS-only intent surface)
- Companion to `adapter_ios_media` on a Linux host, plumbed over MQTT
- Built from the `weave-ios-core` + `nuimo-protocol` Rust crates (UniFFI bindings) — see [`ios/README.md`](ios/README.md) for the Mac toolchain setup

## Crates

This workspace publishes 5 crates to crates.io. Three are SDKs depended on by `weave-server` and the iOS app, one ships the binary, and one is the wire contract:

| Crate | Role |
|---|---|
| `weave-contracts` | WS protocol types (`ServerToEdge` / `EdgeToServer` / `UiFrame` / `Mapping` / `Glyph`). Shared with `weave-server`. |
| `edge-core` | Routing engine (`InputPrimitive` → `Intent`), `ServiceAdapter` trait, WS client. Pure; no service-specific code. |
| `nuimo-protocol` | Nuimo wire-format parsers (BLE GATT shapes + LED matrix encoding). Backend-agnostic so the same logic powers the desktop `nuimo` SDK and the iOS BLE stack. |
| `weave-ios-core` | iOS-specific edge runtime — assembles the routing engine + UniFFI bindings used by the SwiftUI app under `ios/`. |
| `edge-agent` | The binary. Cargo features (`roon` default, `hue`, `macos`) gate the adapter modules under `src/{adapter_roon,adapter_hue,adapter_macos}/`. |

## Install

From crates.io (Linux and macOS):

```sh
cargo install edge-agent                          # default: roon adapter only
cargo install edge-agent --features hue           # + Philips Hue + Hue Tap Dial input
cargo install edge-agent --features macos         # + adapter_macos (MQTT to macos-hub)
cargo install edge-agent --features hue,macos     # both
```

**Linux prerequisites** (BLE needs system packages):

```sh
sudo apt-get install -y libdbus-1-dev pkg-config libssl-dev
```

**macOS** (running edge-agent itself, e.g. on the Roon-Core machine): no extra packages. CoreBluetooth is system-provided. First interactive run triggers the Bluetooth permission dialog — approve in System Settings → Privacy & Security → Bluetooth.

## Config locations

edge-agent separates **code** (this repo) from **per-host config** (never committed here) from **runtime state** (tokens, caches).

| Layer | Path | Managed by |
|---|---|---|
| Template | `docs/config-example.toml` in this repo | git |
| Per-host config | `$XDG_CONFIG_HOME/edge-agent/config.toml` (default `~/.config/edge-agent/config.toml`), or `/etc/edge-agent/config.toml` | you (or a config-management tool like mitamae) |
| Runtime state | `$XDG_STATE_HOME/edge-agent/` — Roon token, Hue token, offline config cache | agent at runtime |

Config path precedence at startup:

1. CLI positional argument: `edge-agent /path/to/config.toml`
2. `EDGE_AGENT_CONFIG` environment variable
3. `$XDG_CONFIG_HOME/edge-agent/config.toml` (falls back to `~/.config/edge-agent/config.toml`)
4. `/etc/edge-agent/config.toml`

If none exist, the agent exits with the list of paths it searched.

Any field in the TOML can be overridden at runtime with `EDGE_AGENT_*` env vars (e.g. `EDGE_AGENT_EDGE_ID`, `EDGE_AGENT_ROON_HOST`, `EDGE_AGENT_NUIMO_BLE_ADDRESSES`).

### Multi-Nuimo

`[nuimo].ble_addresses = ["AA:BB:CC:DD:EE:FF", "11:22:33:44:55:66"]` enrols multiple Nuimo controllers on the same edge. The supervisor scans BLE forever (so a Nuimo powered on after edge-agent startup is picked up automatically), tracks each device by address, and runs an independent event loop, feedback pump, and reconnect cycle per device — Nuimo A's BLE drop never blocks Nuimo B. An empty list (or omitting the key) means "accept any Nuimo discovered" for backward compat with single-device deployments. `nuimo.skip = true` is the explicit BLE opt-out (WS-only witness mode).

### Hue Tap Dial

When `--features hue` is enabled, `adapter_hue` enumerates Hue Tap Dial Switches paired to the same bridge it controls and surfaces them as `device_type = "hue_tap_dial"` controllers. The 4 buttons emit `InputPrimitive::Button { id: 1..=4 }` (route input strings `button_1` through `button_4`); the rotary emits the same `Rotate { delta }` Nuimo does. No extra config — bridge enumeration is the source of truth. The Web UI's Mapping editor lists `hue_tap_dial` as a device_type alongside `nuimo`.

## Running (Linux)

Native — not Docker (BLE needs host bluez/D-Bus).

```sh
# One-time: drop the config template in the XDG location and edit it.
mkdir -p ~/.config/edge-agent
curl -L https://raw.githubusercontent.com/shin1ohno/edge-agent/main/docs/config-example.toml \
  -o ~/.config/edge-agent/config.toml
$EDITOR ~/.config/edge-agent/config.toml

# Run.
RUST_LOG=info edge-agent
```

First-time: approve the extension in Roon → Settings → Extensions. The token is persisted at `~/.local/state/edge-agent/roon-token-${edge_id}.json` and survives restarts.

weave-server ships in the [weave](https://github.com/shin1ohno/weave) `compose.yml` — `docker compose up -d` gets you weave-server (port 3101) + weave-web (port 3100) + mosquitto + roon-hub.

## Running (macOS)

`nuimo-rs` picks `btleplug` (CoreBluetooth) via `cfg(target_os = "macos")`, so the same `cargo build` works on a Mac — no extra flags.

```sh
# Prerequisites
brew install rustup-init && rustup-init -y
source "$HOME/.cargo/env"
rustup default stable

# Install (with Hue + macOS adapters)
cargo install edge-agent --features hue,macos

# Drop the config template in the XDG location and edit it.
mkdir -p ~/.config/edge-agent
curl -L https://raw.githubusercontent.com/shin1ohno/edge-agent/main/docs/config-example.toml \
  -o ~/.config/edge-agent/config.toml
$EDITOR ~/.config/edge-agent/config.toml   # enable [hue] and/or [macos] sections

# Pair your Hue bridge (one-time). Token lands at
# ~/.local/state/edge-agent/hue-token.json by default.
edge-agent pair-hue

# First interactive run — macOS will ask for Bluetooth permission.
RUST_LOG=info edge-agent
# → macOS prompts "edge-agent would like to use Bluetooth" the first time.
#   Approve in System Settings > Privacy & Security > Bluetooth.

# After permission is granted, persist with launchd
cp packaging/macos/com.shin1ohno.edge-agent.plist ~/Library/LaunchAgents/
# Edit the copy: replace __USER__ with your short username, fill EnvironmentVariables
launchctl load ~/Library/LaunchAgents/com.shin1ohno.edge-agent.plist

# Logs
tail -f /tmp/edge-agent.log /tmp/edge-agent.err.log
```

If you also want this Mac to expose its system audio (Core Audio output switching, MediaRemote play/pause, system volume) to other edges, install `macos-hub` separately — see [`companions/mac/macos-hub`](companions/mac/macos-hub) and the `macos-hub` cookbook in [`setup`](https://github.com/shin1ohno/setup).

Notes:
- On macOS, `DiscoveredNuimo.address` is a CoreBluetooth peripheral UUID, not a BLE MAC. The rest of the API is identical to Linux.
- Roon Extension registration is per-machine. The Mac edge-agent registers with a different `extension_id` than your Linux edge, so approve it once in Roon Settings → Extensions.
- launchd will not surface the Bluetooth permission dialog; always do the first run interactively.

## Which path should I use?

| Scenario | Recommended path |
|---|---|
| Home, Roon Core + ≤ 5 devices, each Nuimo near one host | **edge-agent direct** |
| Multiple Nuimos on the same Linux host | **edge-agent direct** (multi-Nuimo supervisor) |
| Mac is the speaker host, Linux owns Roon + Nuimos | **edge-agent direct** + `macos-hub` bridge |
| Hue Tap Dial as input | **edge-agent direct** with `--features hue` |
| iPad as a portable edge controlling Apple Music | edge-agent direct (Linux side) + WeaveIos app |
| Multiple independent dashboards that all want live state | edge-agent + weave-web |
| Devices and services on different hosts (e.g. host A's dial → host B's lights) | **MQTT path** (`roon-hub` + `nuimo-mqtt` + `weave-engine`) — still works |
| You care about <10 ms Nuimo→Roon latency | **edge-agent direct** |

Both paths share the same `weave-server` + SQLite config store. You can run them side by side; they use different Roon extension IDs.

## Offline resilience

edge-agent caches the last `ConfigFull` (mappings + glyphs) at `$XDG_STATE_HOME/edge-agent/config-cache-${edge_id}.json`. If weave-server is down at startup, it loads the cache and keeps routing. Reconnect happens automatically in the background; when the server reappears, it sends a fresh snapshot.

For Hue specifically, the bridge resolver caches the discovered IP keyed by `bridge_id` (MAC-derived) and falls through `stored host → mDNS → Philips cloud discovery` on every connect. DHCP rotation no longer requires re-pairing.

## Status

Production: Roon, Hue (lights + Tap Dial), macOS audio (via macos-hub), iPad (via WeaveIos + adapter_ios_media), multi-Nuimo on Linux, full upstream Nuimo gesture vocabulary (button + rotate + 4 swipes + 4 touches + 4 long-touches + 2 fly + hover proximity).
