# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Initial Cargo workspace and crate scaffold (`openterface-core`,
  `openterface-cli`, `openterface-gui`, `openterface-test-support`).
- Device-agnostic interface contracts: `SerialTransport`, `VideoSource`,
  `DeviceScanner`; the `InputEvent` model; CH9329 frame/checksum primitives;
  the pacing-config surface; and Openterface USB identity constants.
- Hardware-free test doubles (`MockSerial`, `SimulatedVideoSource`,
  `FixtureScanner`).
- CI (fmt, clippy, build, test, supply-chain) and the Apache-2.0 license.
