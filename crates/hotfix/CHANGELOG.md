# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.1](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.11.0...hotfix-v0.11.1) - 2026-04-20

### Added

- add validation config and provide ability to disable orig sent time for admin messages ([#334](https://github.com/Validus-Risk-Management/hotfix/pull/334))

### Other

- remove remaining pattern matching in session code ([#330](https://github.com/Validus-Risk-Management/hotfix/pull/330))

### Added
 - skip check for original sending time(tag 122) in sequence reset messages ([#322](https://github.com/Validus-Risk-Management/hotfix/pull/334))

## [0.11.0](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.10.0...hotfix-v0.11.0) - 2026-03-25

### Added

- notify application of state changes ([#326](https://github.com/Validus-Risk-Management/hotfix/pull/326))

### Other

- centralise message verification and extract shared inbound handlers ([#325](https://github.com/Validus-Risk-Management/hotfix/pull/325))
- break message verification handling out into free function ([#324](https://github.com/Validus-Risk-Management/hotfix/pull/324))
- start breaking out issue handling into their own inbound module ([#323](https://github.com/Validus-Risk-Management/hotfix/pull/323))
- convert resend logic to free functions outside the session code ([#322](https://github.com/Validus-Risk-Management/hotfix/pull/322))
- introduce SessionCtx to hold non-state-machine state ([#321](https://github.com/Validus-Risk-Management/hotfix/pull/321))
- break out session state variants into their own modules ([#319](https://github.com/Validus-Risk-Management/hotfix/pull/319))

## [0.10.0](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.9.1...hotfix-v0.10.0) - 2026-02-24

### Fixed

- resolve deadlock when both sides send resendrequest simultaneously ([#314](https://github.com/Validus-Risk-Management/hotfix/pull/314))

## [0.9.1](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.9.0...hotfix-v0.9.1) - 2026-02-09

### Other

- update readme ([#307](https://github.com/Validus-Risk-Management/hotfix/pull/307))

## [0.9.0](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.8.0...hotfix-v0.9.0) - 2026-02-06

### Added

- allow errors in message parsing and support business reject outcomes ([#304](https://github.com/Validus-Risk-Management/hotfix/pull/304))
- replace anyhow errors in session layer with proper error variants ([#302](https://github.com/Validus-Risk-Management/hotfix/pull/302))

## [0.8.0](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.7.2...hotfix-v0.8.0) - 2026-02-03

### Added

- return confirmation for sent app messages ([#299](https://github.com/Validus-Risk-Management/hotfix/pull/299))

## [0.7.2](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.7.1...hotfix-v0.7.2) - 2026-01-30

### Other

- updated the following local packages: hotfix-store-mongodb

## [0.7.1](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.7.0...hotfix-v0.7.1) - 2026-01-30

### Other

- release ([#293](https://github.com/Validus-Risk-Management/hotfix/pull/293))

## [0.7.0](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.6.0...hotfix-v0.7.0) - 2026-01-29

### Added

- break out message stores into their own crates ([#290](https://github.com/Validus-Risk-Management/hotfix/pull/290))

## [0.6.0](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.5.1...hotfix-v0.6.0) - 2026-01-27

### Added

- add helper function to delete old sequences in MongoDB store ([#286](https://github.com/Validus-Risk-Management/hotfix/pull/286))

### Other

- remove anyhow from hotfix-message crate ([#285](https://github.com/Validus-Risk-Management/hotfix/pull/285))

## [0.5.1](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.5.0...hotfix-v0.5.1) - 2026-01-23

### Added

- replace anyhow with proper error variants in message stores ([#279](https://github.com/Validus-Risk-Management/hotfix/pull/279))

## [0.5.0](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.4.3...hotfix-v0.5.0) - 2026-01-21

### Added

- forbid unwraps and expects in main hotfix crate ([#272](https://github.com/Validus-Risk-Management/hotfix/pull/272))

## [0.4.3](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.4.2...hotfix-v0.4.3) - 2026-01-20

### Other

- Replace unwraps with anyhow errors in session code ([#265](https://github.com/Validus-Risk-Management/hotfix/pull/265))

## [0.4.2](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.4.1...hotfix-v0.4.2) - 2025-12-09

### Added

- decouple hotfix session layer from FIX 4.4 ([#257](https://github.com/Validus-Risk-Management/hotfix/pull/257))

## [0.4.1](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.4.0...hotfix-v0.4.1) - 2025-12-08

### Added

- support non-gap-fill sequence resets ([#255](https://github.com/Validus-Risk-Management/hotfix/pull/255))

## [0.4.0](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.3.2...hotfix-v0.4.0) - 2025-12-08

### Added

- support logout timeouts ([#252](https://github.com/Validus-Risk-Management/hotfix/pull/252))

### Other

- revise use of logout_and_terminate ([#253](https://github.com/Validus-Risk-Management/hotfix/pull/253))

## [0.3.2](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.3.1...hotfix-v0.3.2) - 2025-11-28

### Added

- support reconnects in shutdowns initiated via CLI tool ([#250](https://github.com/Validus-Risk-Management/hotfix/pull/250))

## [0.3.1](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.3.0...hotfix-v0.3.1) - 2025-11-26

### Added

- support admin actions through HTTP interface ([#244](https://github.com/Validus-Risk-Management/hotfix/pull/244))

## [0.3.0](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.2.9...hotfix-v0.3.0) - 2025-11-26

### Added

- allow one off restarts with sequence number reset ([#241](https://github.com/Validus-Risk-Management/hotfix/pull/241))

## [0.2.9](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.2.8...hotfix-v0.2.9) - 2025-11-24

### Added

- better handling of resend requests ([#237](https://github.com/Validus-Risk-Management/hotfix/pull/237))

### Other

- add session-level test case for OrigSendingTime missing in dup message ([#238](https://github.com/Validus-Risk-Management/hotfix/pull/238))
- add test case for processing correct duplicate message ([#235](https://github.com/Validus-Risk-Management/hotfix/pull/235))

## [0.2.8](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.2.7...hotfix-v0.2.8) - 2025-11-19

### Added

- improved Application approach ([#232](https://github.com/Validus-Risk-Management/hotfix/pull/232))
- handle missing and inaccurate SendingTime ([#229](https://github.com/Validus-Risk-Management/hotfix/pull/229))

### Other

- add test case for responding to Test Requests with heartbeat ([#227](https://github.com/Validus-Risk-Management/hotfix/pull/227))

## [0.2.7](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.2.6...hotfix-v0.2.7) - 2025-11-12

### Added

- fix handling of groups and components in message parser ([#226](https://github.com/Validus-Risk-Management/hotfix/pull/226))

## [0.2.6](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.2.5...hotfix-v0.2.6) - 2025-11-03

### Added

- make initiators cloneable ([#223](https://github.com/Validus-Risk-Management/hotfix/pull/223))

## [0.2.5](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.2.4...hotfix-v0.2.5) - 2025-10-21

### Added

- implement QuickFIX-style file message store ([#215](https://github.com/Validus-Risk-Management/hotfix/pull/215))

## [0.2.4](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.2.3...hotfix-v0.2.4) - 2025-10-13

### Added

- correct handling of missing and incorrect OrigSendingTime values ([#211](https://github.com/Validus-Risk-Management/hotfix/pull/211))

## [0.2.3](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.2.2...hotfix-v0.2.3) - 2025-10-09

### Other

- upgrade dependencies to latest version ([#209](https://github.com/Validus-Risk-Management/hotfix/pull/209))

## [0.2.2](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.2.1...hotfix-v0.2.2) - 2025-09-25

### Added

- handle invalid message types by sending a reject ([#202](https://github.com/Validus-Risk-Management/hotfix/pull/202))

## [0.2.1](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.2.0...hotfix-v0.2.1) - 2025-09-24

### Other

- release ([#200](https://github.com/Validus-Risk-Management/hotfix/pull/200))

## [0.2.0](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.1.1...hotfix-v0.2.0) - 2025-09-22

### Added

- resolve begin string for new messages from config ([#192](https://github.com/Validus-Risk-Management/hotfix/pull/192))
- handle infinite resend request loops ([#191](https://github.com/Validus-Risk-Management/hotfix/pull/191))

## [0.1.1](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.1.0...hotfix-v0.1.1) - 2025-09-16

### Other

- release ([#187](https://github.com/Validus-Risk-Management/hotfix/pull/187))

## [0.0.27](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.0.26...hotfix-v0.0.27) - 2025-09-15

### Added

- expose Buffer type needed for codegen ([#184](https://github.com/Validus-Risk-Management/hotfix/pull/184))
- handle messages with incorrect BeginString and comp ID ([#181](https://github.com/Validus-Risk-Management/hotfix/pull/181))

### Other

- test case for garbled message handling ([#180](https://github.com/Validus-Risk-Management/hotfix/pull/180))

## [0.0.26](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.0.25...hotfix-v0.0.26) - 2025-09-08

### Added

- rudimentary dashboard for session state ([#175](https://github.com/Validus-Risk-Management/hotfix/pull/175))

### Other

- formalise when and then structure of test actions and assertions ([#171](https://github.com/Validus-Risk-Management/hotfix/pull/171))

## [0.0.25](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.0.24...hotfix-v0.0.25) - 2025-09-01

### Other

- Initial release with changelogs
