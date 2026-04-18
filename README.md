# edge-agent

Device-host agent bridging physical IoT devices (Nuimo, etc.) to home-audio / IoT services (Roon, Hue, etc.) without MQTT. Each edge host runs one `edge-agent` process that talks directly to the services it controls and synchronizes config with a central `weave-server` over WebSocket.

## Architecture

```
[edge-agent]           (per device host)
 ├ device driver       nuimo-rs (BLE) / future streamdeck / huedial ...
 ├ routing engine      input primitive → service intent
 ├ service adapters    adapter-roon / future adapter-hue ... (Cargo features)
 └ ws client           config pull + state push

      │
      │ WebSocket (LAN, no auth)
      ▼

[weave-server] × 1     config + state hub (SQLite + axum)
```

## Crates

- `weave-contracts` — WS protocol types shared with `weave-server`
- `edge-core` — routing engine, adapter trait, WS client, local config cache
- `edge-agent` — binary; BLE discovery + routing + adapter dispatch
- `adapter-roon` — `ServiceAdapter` impl using `roon-api`

## Status

Phase 1 scaffolding. See `~/.claude/plans/enumerated-sleeping-crab.md` for the full plan.
