# HotFIX, an experimental FIX engine

A FIX engine implemented in Rust with `rustls` support.

> **Warning**
>
> HotFIX is an experimental project, currently incomplete and it may never be completed.
> It is not fit for production.

For the time being, HotFIX focuses on implementing the transport and session layer
correctly to create a fully functional engine.

Outside the session layer, HotFIX uses a fork of [ferrumfix](https://github.com/ferrumfix/ferrumfix)
for message encoding and decoding. Eventually, these changes may be backported to `ferrumfix`,
but this is not feasible at this stage due to the pace and experimental nature of
development of HotFIX.
