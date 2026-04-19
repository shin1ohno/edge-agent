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

- `weave-contracts` — WS protocol types (ServerToEdge / EdgeToServer / UiFrame / Mapping / Glyph). Shared with `weave-server` via git/path dep — edge-agent owns the contract definition.
- `edge-core` — routing engine, `ServiceAdapter` trait, WS client, `GlyphRegistry`, local config cache.
- `edge-agent` — binary. Loads TOML bootstrap config, discovers Nuimo, wires routing + adapters, renders feedback glyphs.
- `adapter-roon` — `ServiceAdapter` backed by `roon-api`. Publishes zone state (playback, volume, now_playing).

## Running

Native — not Docker (BLE needs host bluez/D-Bus).

```sh
cargo build --workspace --release

EDGE_AGENT_EDGE_ID=living-room \
  EDGE_AGENT_CONFIG_SERVER_URL=ws://weave-host:3101/ws/edge \
  EDGE_AGENT_ROON_HOST=192.168.1.20 \
  EDGE_AGENT_ROON_PORT=9330 \
  RUST_LOG=info \
  ./target/release/edge-agent configs/example.toml
```

First-time: approve the extension in Roon → Settings → Extensions. The token is persisted at `~/.local/state/edge-agent/roon-token-${edge_id}.json` and survives restarts.

weave-server ships in the [roon-rs](https://github.com/shin1ohno/roon-rs) `compose.yml` — `docker compose up -d` gets you weave-server (port 3101) + weave-web (port 3100) + mosquitto + roon-hub.

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
