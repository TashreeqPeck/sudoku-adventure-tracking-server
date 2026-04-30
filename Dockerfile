# syntax=docker/dockerfile:1

FROM rust:1.86-bookworm AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY Cargo.lock ./
COPY src ./src
# Cache mounts persist on the builder between builds; copy the binary out because
# /app/target is not part of the image layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --locked --release \
    && cp /app/target/release/sa_tracker /app/sa_tracker

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/sa_tracker /usr/local/bin/sa_tracker
COPY public /public
ENV DATA_DIR=/data
ENV STATIC_DIR=/public
ENV PORT=3840
ENV SHEET_SYNC_INTERVAL_MS=86400000
VOLUME ["/data"]
EXPOSE 3840
CMD ["/usr/local/bin/sa_tracker"]
