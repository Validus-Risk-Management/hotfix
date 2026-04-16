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

## Mixing `hotfix::fix44` and `custom_fix`

The example uses stock field constants from `hotfix::fix44::*` (e.g.
`fix44::CL_ORD_ID`) and the new constant from `custom_fix::CLIENT_STRATEGY_ID`.
This is safe because the custom XML didn't change any FIX 4.4 tag — the
`custom_fix` constants for stock tags are bit-for-bit identical to the ones
in `hotfix::fix44`. If you'd prefer a single source of truth, switch your
imports to `custom_fix::*` everywhere.

## Running the example

In one terminal, start the dummy executor:

```shell
cd dummy-executor && go run .
```

In another, from the repo root, run the example:

```shell
cargo run -p custom-fields -- --config examples/custom-fields/config/test-config.toml
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
