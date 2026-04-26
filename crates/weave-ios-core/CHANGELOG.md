# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
