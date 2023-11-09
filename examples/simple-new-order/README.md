# Example initiator using HotFIX

This dummy application demonstrates the current capabilities of HotFIX.
It connects to an acceptor, and sends a hard-coded single order.

It's mainly used for testing session-level message flows, such as
logons, logouts, disconnects, and resends.

## Running the app

You need an acceptor capable of receiving FIX 4.4 messages. If you already have one,
great, otherwise [the QuickFIX/J example executor](https://github.com/quickfix-j/quickfixj/tree/master/quickfixj-examples/executor)
is straightforward to modify for this use-case.

If you need to modify the configuration, you can find this in `./config/test-config.toml`.

To run the application, just use `cargo run`. Pass in the config path and optionally
the log file path as CLI arguments, e.g.

```shell
cargo run -- -c config/test-config.toml
```

Most of HotFIX's logging is `debug` level, so you may want to adjust
the log levels accordingly:

```shell
RUST_LOG=info,hotfix=debug
```
