FROM docker.io/rust:1.96-bookworm AS builder
WORKDIR /app
COPY loop-dto ./loop-dto
COPY srv ./srv
WORKDIR /app/srv
RUN cargo build --release --package loop-worker-svc

FROM docker.io/debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libpq5 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
RUN groupadd --system loop \
    && useradd --system --gid loop --home-dir /app --shell /usr/sbin/nologin loop \
    && mkdir -p /app/log \
    && chown -R loop:loop /app

COPY --from=builder /app/srv/target/release/loop-worker-svc /usr/local/bin/loop-worker-svc

ENV LOOP_LOG_DIR=/app/log \
    LOOP_ENV=standalone

USER loop
ENTRYPOINT ["/usr/local/bin/loop-worker-svc"]
