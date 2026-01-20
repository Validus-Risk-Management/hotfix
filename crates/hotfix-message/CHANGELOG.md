# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.8](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.2.7...hotfix-message-v0.2.8) - 2026-01-20

### Other

- updated the following local packages: hotfix-dictionary, hotfix-dictionary, hotfix-dictionary, hotfix-codegen

## [0.2.7](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.2.6...hotfix-message-v0.2.7) - 2025-12-09

### Added

- decouple hotfix session layer from FIX 4.4 ([#257](https://github.com/Validus-Risk-Management/hotfix/pull/257))

## [0.2.6](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.2.5...hotfix-message-v0.2.6) - 2025-11-24

### Other

- add test case for processing correct duplicate message ([#235](https://github.com/Validus-Risk-Management/hotfix/pull/235))

## [0.2.5](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.2.4...hotfix-message-v0.2.5) - 2025-11-12

### Added

- fix handling of groups and components in message parser ([#226](https://github.com/Validus-Risk-Management/hotfix/pull/226))

## [0.2.4](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.2.3...hotfix-message-v0.2.4) - 2025-11-03

### Added

- make initiators cloneable ([#223](https://github.com/Validus-Risk-Management/hotfix/pull/223))

### Fixed

- switch message field data structure to IndexMap from BTreeMap to maintain insertion order ([#218](https://github.com/Validus-Risk-Management/hotfix/pull/218))

### Other

- refactor dictionary implementation ([#222](https://github.com/Validus-Risk-Management/hotfix/pull/222))

## [0.2.3](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.2.2...hotfix-message-v0.2.3) - 2025-10-21

### Added

- implement QuickFIX-style file message store ([#215](https://github.com/Validus-Risk-Management/hotfix/pull/215))

## [0.2.2](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.2.1...hotfix-message-v0.2.2) - 2025-10-13

### Added

- correct handling of missing and incorrect OrigSendingTime values ([#211](https://github.com/Validus-Risk-Management/hotfix/pull/211))

## [0.2.1](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.2.0...hotfix-message-v0.2.1) - 2025-10-09

### Other

- upgrade dependencies to latest version ([#209](https://github.com/Validus-Risk-Management/hotfix/pull/209))

## [0.2.0](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.1.2...hotfix-message-v0.2.0) - 2025-09-25

### Added

- handle invalid message types by sending a reject ([#202](https://github.com/Validus-Risk-Management/hotfix/pull/202))

## [0.1.2](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.1.1...hotfix-message-v0.1.2) - 2025-09-24

### Other

- Update README and doc comments ([#199](https://github.com/Validus-Risk-Management/hotfix/pull/199))

## [0.1.1](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.1.0...hotfix-message-v0.1.1) - 2025-09-15

### Added

- make hotfix crate versions move independently ([#186](https://github.com/Validus-Risk-Management/hotfix/pull/186))

## [0.0.26](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.0.25...hotfix-message-v0.0.26) - 2025-09-08

### Added

- expose some useful types in hotfix-message ([#173](https://github.com/Validus-Risk-Management/hotfix/pull/173))

## [0.0.25](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-message-v0.0.24...hotfix-message-v0.0.25) - 2025-09-01

### Other

- Initial release with changelogs
