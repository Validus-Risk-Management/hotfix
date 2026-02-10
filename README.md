<div align="center">

# HotFIX

**A FIX engine written in Rust for buy-side applications.**

[![CI](https://github.com/Validus-Risk-Management/hotfix/actions/workflows/ci.yml/badge.svg)](https://github.com/Validus-Risk-Management/hotfix/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/Validus-Risk-Management/hotfix/graph/badge.svg?token=OE58PBL0N6)](https://codecov.io/gh/Validus-Risk-Management/hotfix)
[![crates-badge]](https://crates.io/crates/hotfix)
[![docs-badge]](https://docs.rs/hotfix)
[![Crates.io](https://img.shields.io/crates/l/hotfix)](LICENSE)

</div>

### Overview

HotFIX is a [FIX](https://www.fixtrading.org/standards/) engine implemented in Rust,
focused on buy-side (initiator) workflows. It fully supports FIX 4.4 and the current
focus is on expanding support to other FIX versions. Performance is roughly on par with
various QuickFIX implementations, with long-term plans to optimise further.

### Features & status

- [x] Network layer including TCP transport with optional TLS support using `rustls`
- [x] Message encoding and decoding
- [x] Session-layer supporting the core flows, such as logins, resends, etc.
- [x] Built-in message stores
    - [x] in-memory
    - [x] file-system
    - [x] [mongodb](https://www.mongodb.com/docs/drivers/rust/current/)
- [x] Code-generation for FIX fields from XML specifications
- [x] Web API and CLI for session monitoring and management
- [ ] Code-generation for complete FIX messages from XML specification

### FIX version support

| Version | Status                              |
|---------|-------------------------------------|
| FIX 4.2 | Should work, but currently untested |
| FIX 4.4 | Fully supported                     |
| FIX 5.0 | Planned                             |

### Getting started

The quickest way to see HotFIX in action is with Docker Compose. The
repository includes an [order-entry](https://github.com/Validus-Risk-Management/hotfix/tree/main/examples/order-entry) example
initiator and a dummy acceptor that you can start together:

```shell
docker compose -f example.compose.yml up --build
```

Once both services are running, open
[http://localhost:9881/order](http://localhost:9881/order) to send FIX
orders through the web UI and watch the messages flow in real time.

See the [examples](https://github.com/Validus-Risk-Management/hotfix/tree/main/examples) directory for more details and
additional ways to run.

### Prior Art

The two major influences for HotFIX are QuickFIX and [FerrumFIX](https://ferrumfix.org/).

QuickFIX implementations in various languages (such as [QuickFIX/J](https://quickfixj.org/))
have influenced the design of the transport and session layers. The FIX message logic
builds on QuickFIX XMLs for the specification. People who are familiar with QuickFIX
will find the API familiar.

The FIX message implementation of HotFIX is a fork of FerrumFIX for things like codegen,
parsing the XML specification, defining fields, etc.

### Contributions

If you're on the buy side and working with FIX 4.4, HotFIX should be ready for
your use case.
If you run into any issues, please file a bug report on
[GitHub Issues](https://github.com/Validus-Risk-Management/hotfix/issues).

Contributions towards larger features outside the current roadmap — for example,
extending HotFIX to support acceptor (sell-side) workflows — are very welcome.
For these, open an
[Issue](https://github.com/Validus-Risk-Management/hotfix/issues) or start a
[Discussion](https://github.com/Validus-Risk-Management/hotfix/discussions)
to coordinate.

[crates-badge]: https://img.shields.io/crates/v/hotfix.svg

[docs-badge]: https://docs.rs/hotfix/badge.svg
