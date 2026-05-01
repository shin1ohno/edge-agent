# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.16.3](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.16.2...edge-agent-v0.16.3) - 2026-05-01

### Other

- dedup feedback per (property, target) — keep track scroll alive ([#98](https://github.com/shin1ohno/edge-agent/pull/98))

## [0.16.2](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.16.1...edge-agent-v0.16.2) - 2026-05-01

### Other

- feed cross-edge ServiceState echoes into local feedback pump ([#97](https://github.com/shin1ohno/edge-agent/pull/97))
- cycle-switch optimistic letter reads engine display_name cache ([#96](https://github.com/shin1ohno/edge-agent/pull/96))
- add macos_music adapter (local Music.app control on Mac) ([#95](https://github.com/shin1ohno/edge-agent/pull/95))
- add dual MIT/Apache-2.0 license files

## [0.16.1](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.16.0...edge-agent-v0.16.1) - 2026-05-01

### Other

- forward unmatched intents to weave-server for cross-edge dispatch ([#92](https://github.com/shin1ohno/edge-agent/pull/92))

## [0.16.0](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.15.0...edge-agent-v0.16.0) - 2026-05-01

### Other

- refresh README to reflect 0.14 stack ([#90](https://github.com/shin1ohno/edge-agent/pull/90))
- extract Roon now_playing title from one_line/two_line ([#88](https://github.com/shin1ohno/edge-agent/pull/88))

## [0.15.0](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.14.0...edge-agent-v0.15.0) - 2026-04-30

### Added

- Nuimo LED text feedback (track scroll + cycle-switch letter hint) ([#86](https://github.com/shin1ohno/edge-agent/pull/86))

### Other

- implement playback_glyph + brightness_bar + power_glyph + mute_glyph + pulse, add server-resolved cycle-switch label ([#87](https://github.com/shin1ohno/edge-agent/pull/87))

## [0.14.0](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.13.1...edge-agent-v0.14.0) - 2026-04-28

### Other

- split Fly from Swipe to match upstream Nuimo gesture set ([#80](https://github.com/shin1ohno/edge-agent/pull/80))

## [0.12.0](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.11.0...edge-agent-v0.12.0) - 2026-04-27

### Other

- device-cycle runtime — active filter + cycle gesture handler ([#71](https://github.com/shin1ohno/edge-agent/pull/71))

## [0.10.0](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.9.0...edge-agent-v0.10.0) - 2026-04-26

### Other

- weave-contracts + edges: cross-edge intent forwarding via DispatchIntent ([#67](https://github.com/shin1ohno/edge-agent/pull/67))

## [0.8.0](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.7.0...edge-agent-v0.8.0) - 2026-04-26

### Other

- device control commands (Connect / Disconnect / DisplayGlyph) ([#60](https://github.com/shin1ohno/edge-agent/pull/60))

## [0.7.0](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.6.0...edge-agent-v0.7.0) - 2026-04-26

### Added

- publish periodic edge metrics for /ws/ui dashboards ([#59](https://github.com/shin1ohno/edge-agent/pull/59))

## [0.6.0](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.5.4...edge-agent-v0.6.0) - 2026-04-26

### Other

- route Hue Tap Dial as a first-class input device ([#56](https://github.com/shin1ohno/edge-agent/pull/56))

## [0.5.3](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.5.2...edge-agent-v0.5.3) - 2026-04-24

### Other

- empty ble_addresses now means accept-all, not WS-only ([#39](https://github.com/shin1ohno/edge-agent/pull/39))

## [0.5.2](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.5.1...edge-agent-v0.5.2) - 2026-04-24

### Other

- supervise multiple Nuimos in parallel ([#37](https://github.com/shin1ohno/edge-agent/pull/37))

## [0.5.1](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.5.0...edge-agent-v0.5.1) - 2026-04-24

### Other

- demote diagnostic publish/forward logs to debug ([#35](https://github.com/shin1ohno/edge-agent/pull/35))
- macOS audio control via MQTT (macos-hub + adapter_macos) ([#34](https://github.com/shin1ohno/edge-agent/pull/34))
- rustfmt fixups after edge-core extraction
- extract routing/adapter/ws-client out of edge-agent binary

## [0.5.0](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.4.3...edge-agent-v0.5.0) - 2026-04-23

### Other

- add Command and Error frames for UI live stream ([#30](https://github.com/shin1ohno/edge-agent/pull/30))

## [0.4.3](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.4.2...edge-agent-v0.4.3) - 2026-04-22

### Other

- route Hue state updates into the Nuimo LED feedback loop ([#28](https://github.com/shin1ohno/edge-agent/pull/28))

## [0.4.2](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.4.1...edge-agent-v0.4.2) - 2026-04-22

### Other

- damp target-selection rotation so cursor steps per quarter turn
- render VolumeBar feedback for Hue brightness + explicit volume_bar rule ([#25](https://github.com/shin1ohno/edge-agent/pull/25))

## [0.4.1](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.4.0...edge-agent-v0.4.1) - 2026-04-22

### Other

- DeviceState emission + feedback rules + Hue SSE timeout ([#23](https://github.com/shin1ohno/edge-agent/pull/23))

## [0.4.0](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.3.3...edge-agent-v0.4.0) - 2026-04-22

### Other

- weave-contracts + edge-agent: cross-service target candidates ([#21](https://github.com/shin1ohno/edge-agent/pull/21))

## [0.3.3](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.3.2...edge-agent-v0.3.3) - 2026-04-22

### Other

- replay cached light state on every SSE (re)connect ([#19](https://github.com/shin1ohno/edge-agent/pull/19))

## [0.3.2](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.3.1...edge-agent-v0.3.2) - 2026-04-21

### Other

- DHCP-resilient bridge resolution + non-fatal init ([#17](https://github.com/shin1ohno/edge-agent/pull/17))

## [0.3.1](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.3.0...edge-agent-v0.3.1) - 2026-04-21

### Other

- move per-host config to XDG, drop committed configs/ ([#16](https://github.com/shin1ohno/edge-agent/pull/16))
- replay cached state on weave-server reconnect ([#14](https://github.com/shin1ohno/edge-agent/pull/14))

## [0.3.0](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.2.0...edge-agent-v0.3.0) - 2026-04-21

### Other

- mark digit_pair + DIGIT_3X5 as allow(dead_code) until wired
- parametric digit_pair(n: u8) renderer for live number display
- rustfmt collapse CommitSelection arms in tests
- top-down fill for dB zones (max=0), fix clipping to 0
- target selection: enter mode pointing at NEXT candidate, not current
- target selection runtime: SelectionMode state machine + SwitchTarget uplink

## [0.2.0](https://github.com/shin1ohno/edge-agent/compare/edge-agent-v0.1.0...edge-agent-v0.2.0) - 2026-04-20

### Other

- fix routing test literal for new Mapping fields
