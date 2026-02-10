# Order Entry — HotFIX example initiator

An example FIX 4.4 initiator that connects to an acceptor and lets you
send NewOrderSingle (D) messages through a web UI.

## Quick start with Docker Compose

The easiest way to run the example is with the compose file at the
repository root. It starts both the order-entry initiator and a
dummy acceptor so everything works out of the box:

```shell
docker compose -f example.compose.yml up --build
```

Once running, open [http://localhost:9881/order](http://localhost:9881/order)
to send orders and see FIX messages flowing.

The HotFIX status dashboard is also available at
[http://localhost:9881](http://localhost:9881).

## Running locally with Cargo

If you prefer to run outside Docker you need a FIX 4.4 acceptor
listening on the host/port in `config/test-config.toml` (defaults to
`127.0.0.1:9880`).

```shell
cargo run --package order-entry -- -c examples/order-entry/config/test-config.toml
```

Then open [http://localhost:9881/order](http://localhost:9881/order).

Set `RUST_LOG` for detailed output:

```shell
RUST_LOG=info,hotfix=debug
```

## Message store

By default the in-memory message store is used. You can switch to the
file-system store with `--database file`, which persists state to the
working directory.
