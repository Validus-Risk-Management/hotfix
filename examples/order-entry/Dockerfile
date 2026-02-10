ARG RUST_VERSION=1.93.0
ARG APP_NAME=order-entry
FROM rust:${RUST_VERSION}-slim-bullseye AS builder

ARG APP_NAME
WORKDIR /app

RUN --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=bind,source=crates,target=crates \
    --mount=type=bind,source=examples,target=examples \
    --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    <<EOF
set -e
cargo build --locked --release --package $APP_NAME
cp ./target/release/$APP_NAME /bin/$APP_NAME
EOF

FROM debian:bullseye-slim AS runner

ARG APP_NAME
ARG UID=10001
RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    appuser
USER appuser

COPY --from=builder /bin/$APP_NAME /bin/$APP_NAME
COPY examples/$APP_NAME/config /config

ENV APP_NAME=$APP_NAME
CMD /bin/$APP_NAME --config /config/test-config-docker.toml
