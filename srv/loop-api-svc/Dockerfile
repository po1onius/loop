FROM docker.io/rust:1.96-bookworm AS builder
WORKDIR /app
COPY loop-dto ./loop-dto
COPY srv ./srv
WORKDIR /app/srv
RUN cargo build --release --package loop-api-svc

FROM docker.io/debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libpq5 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
RUN groupadd --system loop \
    && useradd --system --gid loop --home-dir /app --shell /usr/sbin/nologin loop \
    && mkdir -p /app/static/templates /app/log /etc/loop/config /etc/loop/secrets \
    && chown -R loop:loop /app /etc/loop

COPY --from=builder /app/srv/target/release/loop-api-svc /usr/local/bin/loop-api-svc
COPY --chown=loop:loop srv/static/templates /app/static/templates

ENV LOOP_CONFIG_FILE=/etc/loop/config/loop-api-svc.toml \
    LOOP_HTTP_ADDR=0.0.0.0:3000 \
    LOOP_LOG_DIR=/app/log \
    LOOP_EMAIL_TEMPLATE_GLOB=/app/static/templates/**/* \
    LOOP_ENV=standalone \
    LOOP_JWT_RSA_PRI_KEY_FILE=/etc/loop/secrets/jwt_private.pem \
    LOOP_JWT_RSA_PUB_KEY_FILE=/etc/loop/secrets/jwt_public.pem

EXPOSE 3000
USER loop
ENTRYPOINT ["/usr/local/bin/loop-api-svc"]
