# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.16.2](https://github.com/shin1ohno/edge-agent/compare/edge-core-v0.16.1...edge-core-v0.16.2) - 2026-05-01

### Other

- feed cross-edge ServiceState echoes into local feedback pump ([#97](https://github.com/shin1ohno/edge-agent/pull/97))
- cycle-switch optimistic letter reads engine display_name cache ([#96](https://github.com/shin1ohno/edge-agent/pull/96))

## [0.15.0](https://github.com/shin1ohno/edge-agent/compare/edge-core-v0.14.0...edge-core-v0.15.0) - 2026-04-30

### Other

- implement playback_glyph + brightness_bar + power_glyph + mute_glyph + pulse, add server-resolved cycle-switch label ([#87](https://github.com/shin1ohno/edge-agent/pull/87))

## [0.14.0](https://github.com/shin1ohno/edge-agent/compare/edge-core-v0.13.1...edge-core-v0.14.0) - 2026-04-28

### Other

- split Fly from Swipe to match upstream Nuimo gesture set ([#80](https://github.com/shin1ohno/edge-agent/pull/80))

## [0.13.1](https://github.com/shin1ohno/edge-agent/compare/edge-core-v0.13.0...edge-core-v0.13.1) - 2026-04-28

### Other

- skip inactive mappings in feedback rule resolver ([#78](https://github.com/shin1ohno/edge-agent/pull/78))

## [0.13.0](https://github.com/shin1ohno/edge-agent/compare/edge-core-v0.12.1...edge-core-v0.13.0) - 2026-04-27

### Other

- weave-contracts + edge-agent: cross-edge service_state echo (iOS) ([#76](https://github.com/shin1ohno/edge-agent/pull/76))

## [0.12.1](https://github.com/shin1ohno/edge-agent/compare/edge-core-v0.12.0...edge-core-v0.12.1) - 2026-04-27

### Other

- detect cycle gesture in iOS Nuimo route path (try_cycle_switch) ([#74](https://github.com/shin1ohno/edge-agent/pull/74))

## [0.12.0](https://github.com/shin1ohno/edge-agent/compare/edge-core-v0.11.0...edge-core-v0.12.0) - 2026-04-27

### Other

- device-cycle runtime — active filter + cycle gesture handler ([#71](https://github.com/shin1ohno/edge-agent/pull/71))

## [0.11.0](https://github.com/shin1ohno/edge-agent/compare/edge-core-v0.10.0...edge-core-v0.11.0) - 2026-04-27

### Other

- device-level Connection cycle protocol additions ([#69](https://github.com/shin1ohno/edge-agent/pull/69))

## [0.10.0](https://github.com/shin1ohno/edge-agent/compare/edge-core-v0.9.0...edge-core-v0.10.0) - 2026-04-26

### Other

- weave-contracts + edges: cross-edge intent forwarding via DispatchIntent ([#67](https://github.com/shin1ohno/edge-agent/pull/67))

## [0.8.0](https://github.com/shin1ohno/edge-agent/compare/edge-core-v0.7.0...edge-core-v0.8.0) - 2026-04-26

### Other

- device control commands (Connect / Disconnect / DisplayGlyph) ([#60](https://github.com/shin1ohno/edge-agent/pull/60))

## [0.6.0](https://github.com/shin1ohno/edge-agent/compare/edge-core-v0.5.4...edge-core-v0.6.0) - 2026-04-26

### Other

- route Hue Tap Dial as a first-class input device ([#56](https://github.com/shin1ohno/edge-agent/pull/56))
- drive Nuimo LED via in-process feedback pump ([#54](https://github.com/shin1ohno/edge-agent/pull/54))

## [0.5.2](https://github.com/shin1ohno/edge-agent/compare/edge-core-v0.5.1...edge-core-v0.5.2) - 2026-04-24

### Other

- supervise multiple Nuimos in parallel ([#37](https://github.com/shin1ohno/edge-agent/pull/37))
