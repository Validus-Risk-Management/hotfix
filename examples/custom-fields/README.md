# Custom Fields — using a custom XML dictionary

This example demonstrates how to use a QuickFIX-style XML dictionary that
extends the bundled FIX 4.4 spec with your own custom fields, exercising both
sides of HotFIX's custom-XML support:

- **Build-time codegen** — `build.rs` runs `hotfix-codegen` against
  `spec/FIX44-custom.xml` to produce typed field constants under a
  `custom_fix` module (e.g. `custom_fix::CLIENT_STRATEGY_ID`).
- **Runtime dictionary validation** — the session loads the same XML at
  startup via `data_dictionary_path` and uses it to validate inbound and
  outbound messages.

The example sends a `NewOrderSingle (D)` carrying `ClientStrategyId=42`
and expects the dummy executor to echo the field on the resulting
`ExecutionReport`s. If the field doesn't round-trip, the example exits
non-zero with a descriptive error.

## The custom XML

`spec/FIX44-custom.xml` is a verbatim copy of the bundled
`crates/hotfix-dictionary/src/resources/quickfix/FIX-4.4.xml` with one
addition: a `<field number="6001" name="ClientStrategyId" type="INT"/>`
in the `<fields>` block, plus an optional reference to it on
`NewOrderSingle` and `ExecutionReport`.

## Using the generated constants

All field constants and typed enums (`Side`, `OrdType`, `OrdStatus`, …) come
from the `custom_fix` module — including the ones for standard FIX 4.4
tags. This keeps the example aligned with a single source of truth: the
custom XML drives both compile-time typing and runtime validation. The
`hotfix::fix44` re-exports are deliberately not used here, so the
`hotfix` dependency in `Cargo.toml` doesn't enable the `fix44` feature.

## Running the example

In one terminal, build and start the dummy executor via the existing compose file:

```shell
docker compose -f example.compose.yml up --build dummy-executor
```

In another, from the repo root, run the example:

```shell
cargo run -p custom-fields
```

Expected log output:

```
INFO custom_fields: waiting for logon (up to 10s)
INFO custom_fields::application: logged on
INFO custom_fields: sending NewOrderSingle ClOrdID=demo-1 ClientStrategyId=42
INFO custom_fields: received ExecutionReport ClOrdID=demo-1 OrdStatus=New ClientStrategyId=Some(42)
INFO custom_fields: received ExecutionReport ClOrdID=demo-1 OrdStatus=Filled ClientStrategyId=Some(42)
INFO custom_fields: order filled, custom field round-tripped successfully
INFO custom_fields: shutting down
```

The example should then exit.
