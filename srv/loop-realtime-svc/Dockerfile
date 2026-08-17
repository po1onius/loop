FROM docker.io/rust:1.96-bookworm AS builder
WORKDIR /app
COPY loop-dto ./loop-dto
COPY srv ./srv
WORKDIR /app/srv
RUN cargo build --release --package loop-realtime-svc

FROM docker.io/debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libpq5 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
RUN groupadd --system loop \
    && useradd --system --gid loop --home-dir /app --shell /usr/sbin/nologin loop \
    && mkdir -p /app/log /etc/loop/secrets \
    && chown -R loop:loop /app /etc/loop

COPY --from=builder /app/srv/target/release/loop-realtime-svc /usr/local/bin/loop-realtime-svc

ENV LOOP_HTTP_ADDR=0.0.0.0:3010 \
    LOOP_LOG_DIR=/app/log \
    LOOP_ENV=standalone \
    LOOP_JWT_RSA_PUB_KEY_FILE=/etc/loop/secrets/jwt_public.pem

EXPOSE 3010
USER loop
ENTRYPOINT ["/usr/local/bin/loop-realtime-svc"]
