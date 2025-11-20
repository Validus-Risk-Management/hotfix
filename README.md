<div align="center">

# HotFIX

**A FIX engine written in Rust for buy-side applications.**

[![CI](https://github.com/Validus-Risk-Management/hotfix/actions/workflows/ci.yml/badge.svg)](https://github.com/Validus-Risk-Management/hotfix/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/Validus-Risk-Management/hotfix/graph/badge.svg?token=OE58PBL0N6)](https://codecov.io/gh/Validus-Risk-Management/hotfix)
[![crates-badge]](https://crates.io/crates/hotfix)
[![docs-badge]](https://docs.rs/hotfix)
[![Crates.io](https://img.shields.io/crates/l/hotfix)](LICENSE)

</div>


> **Warning**
>
> HotFIX is currently in development with frequent breaking changes to the API
> and some features missing.

### Overview

HotFIX is a [FIX](https://www.fixtrading.org/standards/) engine implemented in Rust.
While the ambition is to create a robust, fully compliant, ergonomic and performant engine eventually,
this is a large undertaking.

The near-term goal of HotFIX is to provide a functional and useful engine for the buy-side (initiators),
reaching full support of FIX 4.4 and 5.0 workflows as soon as possible.

### Features & status

- [x] Network layer including TCP transport with optional TLS support using `rustls`
- [x] Message encoding and decoding (FIX 4.4)
- [x] Session-layer supporting the core flows, such as logins, resends, etc.
- [x] Built-in message stores
    - [x] in-memory
    - [x] file-system
    - [x] [mongodb](https://www.mongodb.com/docs/drivers/rust/current/)
    - [x] [redb](https://www.redb.org/)
- [x] Code-generation for FIX fields from XML specifications
- [ ] FIX 5.0 support
- [ ] Code-generation for complete FIX messages from XML specification

Check out the [examples](https://github.com/Validus-Risk-Management/hotfix/tree/main/examples)
to get started.

### Prior Art

The two major influences for HotFIX are QuickFIX and [FerrumFIX](https://ferrumfix.org/).

QuickFIX implementations in various languages (such as [QuickFIX/J](https://quickfixj.org/))
have influenced the design of the transport and session layers. The FIX message logic
builds on QuickFIX XMLs for the specification. People who are familiar with QuickFIX
will find the API familiar.

The FIX message implementation of HotFIX is a fork of FerrumFIX for things like codegen,
parsing the XML specification, defining fields, etc.

### Contributions

In its current state, the engine has a lot of issues that will be fixed
in due course, so please don't create issues or PRs for individual bugs.

We welcome committed contributors who want to work with us to turn this
into a successful project. There are many components that can be developed
in parallel. If you are interested in participating, don't hesitate to
reach out.

The best way to get in touch is by
[starting a Discussion](https://github.com/Validus-Risk-Management/hotfix/discussions).

[crates-badge]: https://img.shields.io/crates/v/hotfix.svg

[docs-badge]: https://docs.rs/hotfix/badge.svg
