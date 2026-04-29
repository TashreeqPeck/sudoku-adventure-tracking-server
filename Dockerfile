# syntax=docker/dockerfile:1

FROM rust:1.85-bookworm AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/sa_tracker /usr/local/bin/sa_tracker
COPY public /public
ENV DATA_DIR=/data
ENV STATIC_DIR=/public
ENV PORT=3840
ENV SHEET_SYNC_INTERVAL_MS=86400000
VOLUME ["/data"]
EXPOSE 3840
CMD ["/usr/local/bin/sa_tracker"]
