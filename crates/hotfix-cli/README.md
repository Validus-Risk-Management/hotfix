<div align="center">

# hotfix-cli

**CLI tool for the HotFIX engine.**

</div>

This crate is part of the [hotfix](https://github.com/hotfix-rs/hotfix) project.

It provides a CLI client for the [web interface](https://crates.io/crates/hotfix-web) of the hotfix FIX engine
which supports fetching session state and sending admin commands to the running session.

## Installation

You can either install the tool using `cargo install hotfix-cli` or use it as a library in your own project.

## How to use it

You need to have a running hotfix FIX engine instance with the web interface exposed using the
[hotfix-web](https://crates.io/crates/hotfix-web) crate.

The tool tries to connect to the web interface using the default address `http://localhost:9881`.
You can override this using either the explicit CLI argument `--url` or the environment variable `HOTFIX_CLI_URL`.

With everything set up, you can use the tool to run commands, e.g.

```shell
hotfix session-info
```

For the full list of available commands, run `hotfix --help`.
