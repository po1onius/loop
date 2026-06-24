SHELL := /usr/bin/env bash
.SHELLFLAGS := -eu -o pipefail -c

.DEFAULT_GOAL := backend-up

COMPOSE ?= podman compose
CARGO ?= cargo
STANDALONE_DIR := deploy/standalone
LOCAL_ENV ?= $(STANDALONE_DIR)/.env
SRV_DIR := srv
# Podman 会将无 registry 的本地镜像规范化为 localhost/...，
# 这里固定完整镜像名，避免名称不一致导致重复构建 migrate 镜像。
MIGRATE_IMAGE := localhost/loop-diesel-cli:standalone

.PHONY: backend-up
backend-up:
	@if [[ ! -f "$(LOCAL_ENV)" ]]; then \
		echo "missing $(LOCAL_ENV)"; \
		echo "copy deploy/standalone/examples/.env.example to $(LOCAL_ENV) and adjust values"; \
		exit 1; \
	fi
	@if ! $(COMPOSE) version >/dev/null 2>&1; then \
		echo "missing compose command: $(COMPOSE)"; \
		echo "install Podman Compose, or override COMPOSE explicitly"; \
		exit 1; \
	fi
	@mkdir -p "$(SRV_DIR)/log"
	@set -a; source "$(LOCAL_ENV)"; set +a; \
	: "$${PGDATABASE:?missing PGDATABASE in $(LOCAL_ENV)}"; \
	: "$${PGUSER:?missing PGUSER in $(LOCAL_ENV)}"; \
	: "$${PGPASSWORD:?missing PGPASSWORD in $(LOCAL_ENV)}"; \
	: "$${LOOP_REDIS_PASSWORD:?missing LOOP_REDIS_PASSWORD in $(LOCAL_ENV)}"; \
	: "$${LOOP_HOST_CONFIG_FILE:?missing LOOP_HOST_CONFIG_FILE in $(LOCAL_ENV)}"; \
	: "$${LOOP_HOST_SECRETS_DIR:?missing LOOP_HOST_SECRETS_DIR in $(LOCAL_ENV)}"; \
	repo_root="$$(pwd -P)"; \
	standalone_dir="$${repo_root}/$(STANDALONE_DIR)"; \
	host_config_file="$${LOOP_HOST_CONFIG_FILE}"; \
	if [[ "$${host_config_file}" != /* ]]; then host_config_file="$${standalone_dir}/$${host_config_file}"; fi; \
	host_secrets_dir="$${LOOP_HOST_SECRETS_DIR}"; \
	if [[ "$${host_secrets_dir}" != /* ]]; then host_secrets_dir="$${standalone_dir}/$${host_secrets_dir}"; fi; \
	host_log_dir="$${LOOP_HOST_LOG_DIR:-../../srv/log}"; \
	if [[ "$${host_log_dir}" != /* ]]; then host_log_dir="$${standalone_dir}/$${host_log_dir}"; fi; \
	[[ -f "$${host_config_file}" ]] || { echo "missing config file: $${host_config_file}"; exit 1; }; \
	[[ -f "$${host_secrets_dir}/jwt_private.pem" ]] || { echo "missing JWT private key: $${host_secrets_dir}/jwt_private.pem"; exit 1; }; \
	[[ -f "$${host_secrets_dir}/jwt_public.pem" ]] || { echo "missing JWT public key: $${host_secrets_dir}/jwt_public.pem"; exit 1; }; \
	mkdir -p "$${host_log_dir}"; \
	( cd "$(STANDALONE_DIR)" && $(COMPOSE) -f compose.yaml up -d db cache minio minio-init otel-collector tempo loki alloy prometheus grafana ); \
	if ! podman image exists "$(MIGRATE_IMAGE)" >/dev/null 2>&1; then \
		echo "missing $(MIGRATE_IMAGE), building migrate image..."; \
		( cd "$(STANDALONE_DIR)" && $(COMPOSE) -f compose.yaml build migrate ); \
	fi; \
	( cd "$(STANDALONE_DIR)" && $(COMPOSE) -f compose.yaml run --rm migrate ); \
	export DATABASE_URL="postgresql://$${PGUSER}:$${PGPASSWORD}@localhost:$${LOOP_PG_PORT:-5132}/$${PGDATABASE}"; \
	export REDIS_URL="redis://:$${LOOP_REDIS_PASSWORD}@localhost:$${LOOP_REDIS_PORT:-6319}"; \
	export LOOP_CONFIG_FILE="$${host_config_file}"; \
	export LOOP_JWT_RSA_PRI_KEY_FILE="$${host_secrets_dir}/jwt_private.pem"; \
	export LOOP_JWT_RSA_PUB_KEY_FILE="$${host_secrets_dir}/jwt_public.pem"; \
	export LOOP_EMAIL_TEMPLATE_GLOB="$${repo_root}/$(SRV_DIR)/static/templates/**/*"; \
	export LOOP_HTTP_ADDR="0.0.0.0:3000"; \
	export LOOP_ENV="$${LOOP_ENV:-local}"; \
	export LOOP_LOG_DIR="$${host_log_dir}"; \
	export OTEL_EXPORTER_OTLP_TRACES_ENDPOINT="$${OTEL_EXPORTER_OTLP_TRACES_ENDPOINT:-http://127.0.0.1:$${LOOP_OTEL_GRPC_PORT:-4317}}"; \
	cd "$(SRV_DIR)" && $(CARGO) run -p loop-api-svc
