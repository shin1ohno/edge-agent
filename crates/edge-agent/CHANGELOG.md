# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
