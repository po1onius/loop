SHELL := /usr/bin/env bash
.SHELLFLAGS := -eu -o pipefail -c

.DEFAULT_GOAL := help

CARGO ?= cargo
COMPOSE ?= podman compose
LOCAL_ENV ?= deploy/local/.env
SERVICE ?= loop-event-svc
SRV_DIR := srv
COMPOSE_FILE := deploy/compose.yaml

.PHONY: help
help:
	@printf '%s\n' \
		'Targets:' \
		'  make dev-event      Run loop-event-svc with deploy/local/.env loaded' \
		'  make deps-up        Start local Postgres, Redis and MinIO from deploy/compose.yaml' \
		'  make deps-down      Stop local compose dependencies' \
		'  make db-migrate     Initialize local Postgres schema with Diesel' \
		'  make db-revert      Revert the current Diesel init schema' \
		'  make db-redo        Recreate the current Diesel init schema' \
		'  make db-status      Show Diesel migration status' \
		'  make dto-gen        Generate TypeScript DTO bindings for app/lib/dto.ts' \
		'  make fmt            Format Rust workspace' \
		'  make fmt-check      Check Rust formatting' \
		'  make check          cargo check for srv workspace' \
		'  make clippy         Run clippy for srv workspace' \
		'  make test-event     Test loop-infra and loop-event-svc' \
		'  make test           Run all srv tests' \
		'  make local-init     Create local config examples if missing'

.PHONY: dev-event
dev-event:
	@if [[ ! -f "$(LOCAL_ENV)" ]]; then \
		echo "missing $(LOCAL_ENV)"; \
		echo "run: make local-init"; \
		echo "then copy deploy/local/.env.example to $(LOCAL_ENV) and adjust values"; \
		exit 1; \
	fi
	@set -a; source "$(LOCAL_ENV)"; set +a; \
	: "$${LOOP_CONFIG_FILE:?missing LOOP_CONFIG_FILE in $(LOCAL_ENV)}"; \
	: "$${LOOP_PG_CONN:?missing LOOP_PG_CONN in $(LOCAL_ENV); quote URLs that contain &}"; \
	: "$${LOOP_REDIS_CONN:?missing LOOP_REDIS_CONN in $(LOCAL_ENV)}"; \
	cd "$(SRV_DIR)" && $(CARGO) run -p loop-event-svc

.PHONY: deps-up
deps-up:
	@$(COMPOSE) -f "$(COMPOSE_FILE)" up -d db cache minio minio-init

.PHONY: deps-down
deps-down:
	@$(COMPOSE) -f "$(COMPOSE_FILE)" down

.PHONY: db-migrate db-revert db-redo db-status
db-migrate: DIESEL_MIGRATION_CMD := run
db-revert: DIESEL_MIGRATION_CMD := revert
db-redo: DIESEL_MIGRATION_CMD := redo
db-status: DIESEL_MIGRATION_CMD := list
db-migrate db-revert db-redo db-status:
	@if [[ ! -f "$(LOCAL_ENV)" ]]; then \
		echo "missing $(LOCAL_ENV)"; \
		echo "run: make local-init"; \
		echo "then copy deploy/local/.env.example to $(LOCAL_ENV) and adjust values"; \
		exit 1; \
	fi
	@if ! command -v diesel >/dev/null 2>&1; then \
		echo "missing diesel CLI"; \
		echo "install: cargo install diesel_cli --no-default-features --features postgres"; \
		exit 1; \
	fi
	@set -a; source "$(LOCAL_ENV)"; set +a; \
	if [[ -z "$${DATABASE_URL:-}" ]]; then \
		: "$${LOOP_PG_CONN:?missing LOOP_PG_CONN or DATABASE_URL in $(LOCAL_ENV); quote URLs that contain &}"; \
		export DATABASE_URL="$${LOOP_PG_CONN}"; \
	fi; \
	cd "$(SRV_DIR)" && diesel migration $(DIESEL_MIGRATION_CMD)

.PHONY: dto-gen
dto-gen:
	@cd loop-dto && $(CARGO) run --quiet --bin export_dto

.PHONY: fmt
fmt:
	@cd "$(SRV_DIR)" && $(CARGO) fmt

.PHONY: fmt-check
fmt-check:
	@cd "$(SRV_DIR)" && $(CARGO) fmt --check

.PHONY: check
check:
	@cd "$(SRV_DIR)" && $(CARGO) check

.PHONY: clippy
clippy:
	@cd "$(SRV_DIR)" && $(CARGO) clippy --all-targets --all-features

.PHONY: test-event
test-event:
	@cd "$(SRV_DIR)" && $(CARGO) test -p loop-infra -p loop-event-svc

.PHONY: test
test:
	@cd "$(SRV_DIR)" && $(CARGO) test

.PHONY: local-init
local-init:
	@mkdir -p deploy/local/secrets
	@if [[ ! -f deploy/local/.env.example ]]; then \
		printf '%s\n' \
			"LOOP_CONFIG_FILE='../deploy/local/loop-event-svc.example.toml'" \
			"LOOP_HTTP_ADDR='127.0.0.1:3000'" \
			"LOOP_PG_CONN='postgresql://localhost:5132/loop?user=srus&password=wdnmd'" \
			"LOOP_REDIS_CONN='redis://localhost:6319'" \
			"LOOP_JWT_RSA_PRI_KEY_FILE='../deploy/local/secrets/jwt_private.pem'" \
			"LOOP_JWT_RSA_PUB_KEY_FILE='../deploy/local/secrets/jwt_public.pem'" \
			"LOOP_SMTP_TOKEN='local-smtp-token'" \
			"LOOP_S3_ACCESS_KEY_ID='loopadmin'" \
			"LOOP_S3_SECRET_ACCESS_KEY='loopadmin123'" \
			> deploy/local/.env.example; \
	fi
	@if [[ ! -f deploy/local/loop-event-svc.example.toml ]]; then \
		printf '%s\n' \
			'access_ttl = 900' \
			'refresh_ttl = 2592000' \
			'' \
			'[perm]' \
			'perm_ver = 1' \
			'' \
			'[perm.role_perm]' \
			'user = ["event.read", "event.join", "event.create", "media.upload", "community.post.create"]' \
			'organizer = ["event.read", "event.join", "event.create", "event.update_own", "media.upload"]' \
			'admin = ["*"]' \
			'' \
			'[email]' \
			'from = "Loop <no-reply@example.com>"' \
			'' \
			'[email.smtp]' \
			'sender = "smtp-user"' \
			'domain = "smtp.example.com"' \
			'' \
			'[storage]' \
			'bucket = "loop-local"' \
			'region = "us-east-1"' \
			'endpoint_url = "http://127.0.0.1:9000"' \
			'public_base_url = "http://127.0.0.1:9000/loop-local"' \
			'key_prefix = "media"' \
			'force_path_style = true' \
			'presign_expires_secs = 900' \
			'max_upload_bytes = 10485760' \
			'allowed_mime_types = ["image/jpeg", "image/png", "image/webp", "image/gif"]' \
			> deploy/local/loop-event-svc.example.toml; \
	fi
	@printf '%s\n' \
		'created local examples under deploy/local/' \
		'copy deploy/local/.env.example to deploy/local/.env and add JWT key files before make dev-event'
