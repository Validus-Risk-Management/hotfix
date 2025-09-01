# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.25](https://github.com/Validus-Risk-Management/hotfix/compare/hotfix-v0.0.24...hotfix-v0.0.25) - 2025-09-01

### Other

- Fix bug in sequence reset resetting to incorrect sequence number
- Individually increment target sequence number in message type specific branches
- Yet another increase to test timeouts
- Fix issue with incorrectly incremented target seq number on logons
- Add message verification to business messages
- Add test for logon timeouts
- Correctly handle peer timeouts when we are awaiting logon
- Add logon timeout as peer timer to session state
- Add logon timeout to config
- Rewrite reject tests to use the fluent API for constructing Reject objects
- Send reject message in response to message with invalid field
- Add reject message type and a failing test for invalid fields being rejected
- Further increase test assertion timeouts
- Autofix clippy issues after version upgrade
- Implement the logon flow with sequence number too high to completion with message recovery
- Add test case for logon response whose sequence number is too high
- Add test case for logon response whose sequence number is too low
- Add business test case using a new order and an execution report
- Disconnect counterparty if they respond with non-logon message to logon
- Refactor setup into helpers using the wording 'given'
- Take idea of when/then wording further
- Experiment with the language used for actions and assertions in session test cases
- Increase default timeout
- Add test case for happy logon flow
- Move setup utilities into common module to share between test modules
- Refactor session tests into multiple modules
- Leverage session info API to improve test case for peer timeout
- Add API to get information on the session
