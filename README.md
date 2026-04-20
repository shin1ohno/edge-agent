# edge-agent

Device-host agent bridging physical IoT devices (Nuimo today, more later) to home-audio / IoT services (Roon today, more later) **without MQTT**. Each edge host runs one `edge-agent` binary that talks directly to the services it controls, and syncs its config and state with a central `weave-server` over WebSocket.

## Why this exists

At home scale — one Roon Core, a handful of devices, each Nuimo physically near one host — MQTT plus a routing engine in the middle is overkill. This agent removes two hops:

- **Before**: `nuimo-mqtt` → MQTT broker → `weave-engine` → MQTT broker → `roon-hub` → Roon Core.
- **After**: `edge-agent` (one binary, one host) → Roon Core.

The MQTT path (via `roon-hub` + `nuimo-mqtt` + `weave-engine`) is still maintained — it's better when devices and services span multiple hosts (N:N cross-host). See the "Which path?" section below.

## Architecture

```
[edge-agent] × N  (per device host)
 ├ device driver     nuimo (BLE) today; streamdeck / huedial later
 ├ routing engine    input primitive → service intent (config-driven)
 ├ service adapters  adapter-roon today; adapter-hue later (Cargo features)
 └ ws client         /ws/edge: config pull (ConfigFull/Patch, GlyphsUpdate) + state push

                          │ WebSocket, LAN, no auth
                          ▼

[weave-server]  config + state hub (SQLite + axum + /ws/edge + /ws/ui + Next.js Web UI)
```

Each edge-agent registers as its own Roon Extension (unique `extension_id`) so multiple edges coexist cleanly under Roon's own account model.

## Crates

- `weave-contracts` — WS protocol types (ServerToEdge / EdgeToServer / UiFrame / Mapping / Glyph), shared with `weave-server` via crates.io.
- `edge-agent` — binary. Absorbs the former `edge-core` / `adapter-roon` / `adapter-hue` crates as internal modules (see `src/{edge_core,adapter_roon,adapter_hue}/`), so `cargo install edge-agent` pulls a single self-contained crate from crates.io.

## Install

From crates.io (Linux and macOS):

```sh
cargo install edge-agent                 # default: roon adapter only
cargo install edge-agent --features hue  # + Philips Hue
```

**Linux prerequisites** (BLE needs system packages):

```sh
sudo apt-get install -y libdbus-1-dev pkg-config libssl-dev
```

**macOS**: no extra packages. CoreBluetooth is system-provided. First interactive run will trigger the Bluetooth permission dialog — approve in System Settings → Privacy & Security → Bluetooth.

## Running (Linux)

Native — not Docker (BLE needs host bluez/D-Bus).

```sh
EDGE_AGENT_EDGE_ID=living-room \
  EDGE_AGENT_CONFIG_SERVER_URL=ws://weave-host:3101/ws/edge \
  EDGE_AGENT_ROON_HOST=192.168.1.20 \
  EDGE_AGENT_ROON_PORT=9330 \
  RUST_LOG=info \
  edge-agent configs/example.toml
```

First-time: approve the extension in Roon → Settings → Extensions. The token is persisted at `~/.local/state/edge-agent/roon-token-${edge_id}.json` and survives restarts.

weave-server ships in the [roon-rs](https://github.com/shin1ohno/roon-rs) `compose.yml` — `docker compose up -d` gets you weave-server (port 3101) + weave-web (port 3100) + mosquitto + roon-hub.

## Running (macOS)

`nuimo-rs` picks `btleplug` (CoreBluetooth) via `cfg(target_os = "macos")`, so the same `cargo build` works on a Mac — no extra flags.

```sh
# Prerequisites
brew install rustup-init && rustup-init -y
source "$HOME/.cargo/env"
rustup default stable

# Install
cargo install edge-agent --features hue

# Pair your Hue bridge (one-time)
edge-agent pair-hue

# First interactive run — macOS will ask for Bluetooth permission
EDGE_AGENT_EDGE_ID=mac-living \
  EDGE_AGENT_CONFIG_SERVER_URL=ws://weave.lan:3101/ws/edge \
  EDGE_AGENT_ROON_HOST=192.168.1.20 EDGE_AGENT_ROON_PORT=9330 \
  EDGE_AGENT_HUE_TOKEN_PATH="$HOME/Library/Application Support/edge-agent/hue-token.json" \
  edge-agent configs/example.toml
# → macOS prompts "edge-agent would like to use Bluetooth" the first time.
#   Approve in System Settings > Privacy & Security > Bluetooth.

# After permission is granted, persist with launchd
cp packaging/macos/com.shin1ohno.edge-agent.plist ~/Library/LaunchAgents/
# Edit the copy: replace __USER__ with your short username, fill EnvironmentVariables
launchctl load ~/Library/LaunchAgents/com.shin1ohno.edge-agent.plist

# Logs
tail -f /tmp/edge-agent.log /tmp/edge-agent.err.log
```

Notes:
- On macOS, `DiscoveredNuimo.address` is a CoreBluetooth peripheral UUID, not a BLE MAC. The rest of the API is identical to Linux.
- Roon Extension registration is per-machine. The Mac edge-agent registers with a different `extension_id` than your Linux edge, so approve it once in Roon Settings → Extensions.
- Launchd will not surface the Bluetooth permission dialog; always do the first run interactively.

## Which path should I use?

| Scenario | Recommended path |
|---|---|
| Home, Roon Core + ≤ 5 devices, each Nuimo near one host | **edge-agent direct** |
| Multiple independent dashboards that all want live state | edge-agent + weave-web |
| Devices and services on different hosts (e.g. host A's dial → host B's lights) | **MQTT path** (`roon-hub` + `nuimo-mqtt` + `weave-engine`) |
| You care about <10 ms Nuimo→Roon latency | **edge-agent direct** |
| You run many dashboards / want retained-message semantics | MQTT path |

Both paths share the same `weave-server` + SQLite config store. You can run them side by side; they use different Roon extension IDs.

## Offline resilience

edge-agent caches the last `ConfigFull` (including mappings and glyphs) at `$XDG_STATE_HOME/edge-agent/config-cache-${edge_id}.json`. If the weave-server is down at startup, it loads the cache and keeps routing. Reconnect happens automatically in the background; when the server reappears, it sends a fresh snapshot.

## Status

Phase 1 + Phase 2 + Phase 3 complete. See `~/.claude/plans/enumerated-sleeping-crab.md` for the full plan history.
