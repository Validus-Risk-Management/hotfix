<div align="center">

# hotfix-http

**Management endpoints and UI for the HotFIX engine.**

</div>

This crate is an add-on for the [HotFIX engine](https://github.com/Validus-Risk-Management/hotfix)
to provide useful APIs for admin actions, retrieving FIX session state and health information.

Optionally, it also provides a web-based UI to view and manage the session state.

## Usage

`hotfix-http` build an `axum` router you can embed in your application in any way you like.

To build the router, just call `build_router` with the HotFIX session ref:

```rust
use hotfix_status::build_router;

async fn start_status_service(session_ref: SessionRef<Message>) {
    let status_router = build_router(session_ref);
    let host_and_port = std::env::var("HOST_AND_PORT").unwrap_or("0.0.0.0:9881".to_string());
    let listener = tokio::net::TcpListener::bind(&host_and_port).await.unwrap();

    info!("starting status service on http://{host_and_port}");
    axum::serve(listener, status_router).await.unwrap();
}
```

For a full example, check out
the [simple-new-order](https://github.com/Validus-Risk-Management/hotfix/tree/main/examples/simple-new-order)
sample application.
