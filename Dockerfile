FROM rust:1.86 as builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --bin redis-lite-server

FROM debian:bookworm-slim
WORKDIR /app

RUN useradd -m -u 10001 redislite

COPY --from=builder /app/target/release/redis-lite-server /usr/local/bin/redis-lite-server

RUN mkdir -p /data && chown -R redislite:redislite /data
USER redislite

EXPOSE 6379
VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/redis-lite-server"]
CMD ["--bind", "0.0.0.0:6379", "--data-file", "/data/data.json", "--aof-file", "/data/appendonly.aof", "--autoload", "--appendonly", "--autosave"]
