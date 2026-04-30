# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.15.0](https://github.com/shin1ohno/edge-agent/compare/weave-ios-core-v0.14.0...weave-ios-core-v0.15.0) - 2026-04-30

### Added

- Nuimo LED text feedback (track scroll + cycle-switch letter hint) ([#86](https://github.com/shin1ohno/edge-agent/pull/86))

### Other

- implement playback_glyph + brightness_bar + power_glyph + mute_glyph + pulse, add server-resolved cycle-switch label ([#87](https://github.com/shin1ohno/edge-agent/pull/87))
- read Nuimo battery on connect ([#82](https://github.com/shin1ohno/edge-agent/pull/82))

## [0.13.0](https://github.com/shin1ohno/edge-agent/compare/weave-ios-core-v0.12.1...weave-ios-core-v0.13.0) - 2026-04-27

### Other

- weave-contracts + edge-agent: cross-edge service_state echo (iOS) ([#76](https://github.com/shin1ohno/edge-agent/pull/76))

## [0.12.1](https://github.com/shin1ohno/edge-agent/compare/weave-ios-core-v0.12.0...weave-ios-core-v0.12.1) - 2026-04-27

### Other

- detect cycle gesture in iOS Nuimo route path (try_cycle_switch) ([#74](https://github.com/shin1ohno/edge-agent/pull/74))

## [0.12.0](https://github.com/shin1ohno/edge-agent/compare/weave-ios-core-v0.11.0...weave-ios-core-v0.12.0) - 2026-04-27

### Other

- device-cycle runtime — active filter + cycle gesture handler ([#71](https://github.com/shin1ohno/edge-agent/pull/71))

## [0.11.0](https://github.com/shin1ohno/edge-agent/compare/weave-ios-core-v0.10.0...weave-ios-core-v0.11.0) - 2026-04-27

### Other

- device-level Connection cycle protocol additions ([#69](https://github.com/shin1ohno/edge-agent/pull/69))

## [0.10.0](https://github.com/shin1ohno/edge-agent/compare/weave-ios-core-v0.9.0...weave-ios-core-v0.10.0) - 2026-04-26

### Other

- weave-contracts + edges: cross-edge intent forwarding via DispatchIntent ([#67](https://github.com/shin1ohno/edge-agent/pull/67))

## [0.9.0](https://github.com/shin1ohno/edge-agent/compare/weave-ios-core-v0.8.1...weave-ios-core-v0.9.0) - 2026-04-26

### Other

- switch DeviceControlSink to sync trait for Swift 6 ([#65](https://github.com/shin1ohno/edge-agent/pull/65))

## [0.8.1](https://github.com/shin1ohno/edge-agent/compare/weave-ios-core-v0.8.0...weave-ios-core-v0.8.1) - 2026-04-26

### Other

- route DeviceConnect / DeviceDisconnect / DisplayGlyph to Swift ([#62](https://github.com/shin1ohno/edge-agent/pull/62))

## [0.8.0](https://github.com/shin1ohno/edge-agent/compare/weave-ios-core-v0.7.0...weave-ios-core-v0.8.0) - 2026-04-26

### Other

- device control commands (Connect / Disconnect / DisplayGlyph) ([#60](https://github.com/shin1ohno/edge-agent/pull/60))

## [0.7.0](https://github.com/shin1ohno/edge-agent/compare/weave-ios-core-v0.6.0...weave-ios-core-v0.7.0) - 2026-04-26

### Added

- publish periodic edge metrics for /ws/ui dashboards ([#59](https://github.com/shin1ohno/edge-agent/pull/59))

### Other

- throttle LED feedback per (device, property), not per device ([#57](https://github.com/shin1ohno/edge-agent/pull/57))

## [0.6.0](https://github.com/shin1ohno/edge-agent/compare/weave-ios-core-v0.5.4...weave-ios-core-v0.6.0) - 2026-04-26

### Other

- drive Nuimo LED via in-process feedback pump ([#54](https://github.com/shin1ohno/edge-agent/pull/54))
- drive system volume + mute via MPVolumeView slider trick ([#52](https://github.com/shin1ohno/edge-agent/pull/52))
- forward Apple Music Now Playing snapshots to weave-server ([#50](https://github.com/shin1ohno/edge-agent/pull/50))
- dispatch transport intents to Apple Music via MPMusicPlayerController ([#48](https://github.com/shin1ohno/edge-agent/pull/48))
- ingest mappings into a local routing engine ([#45](https://github.com/shin1ohno/edge-agent/pull/45))

## [0.5.4](https://github.com/shin1ohno/edge-agent/compare/weave-ios-core-v0.5.3...weave-ios-core-v0.5.4) - 2026-04-25

### Other

- forward Nuimo input events as device_state so DevicesPane reacts ([#44](https://github.com/shin1ohno/edge-agent/pull/44))
- announce iPad as edge over /ws/edge ([#42](https://github.com/shin1ohno/edge-agent/pull/42))
