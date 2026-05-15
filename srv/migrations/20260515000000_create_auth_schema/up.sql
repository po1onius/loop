-- Initial authentication schema.
-- This migration is intentionally idempotent enough to baseline early local
-- databases that were initialized before migrations became the schema source.

CREATE TABLE IF NOT EXISTS users (
    user_id BIGSERIAL PRIMARY KEY,
    username TEXT NOT NULL,
    account TEXT NOT NULL,
    pwd TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user'
);

DO $$
BEGIN
    ALTER TABLE users ADD CONSTRAINT users_account_key UNIQUE (account);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE users ADD CONSTRAINT users_username_not_empty CHECK (length(btrim(username)) > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE users ADD CONSTRAINT users_username_len CHECK (char_length(username) <= 50);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE users ADD CONSTRAINT users_account_not_empty CHECK (length(btrim(account)) > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE users ADD CONSTRAINT users_account_len CHECK (char_length(account) <= 100);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE users ADD CONSTRAINT users_pwd_not_empty CHECK (length(pwd) > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE users ADD CONSTRAINT users_role_not_empty CHECK (length(btrim(role)) > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    token_hash TEXT NOT NULL,
    device_id TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ip_address INET,
    user_agent TEXT
);

DO $$
BEGIN
    ALTER TABLE refresh_tokens ADD CONSTRAINT refresh_tokens_token_hash_key UNIQUE (token_hash);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE refresh_tokens
        ADD CONSTRAINT refresh_tokens_token_hash_not_empty CHECK (length(btrim(token_hash)) > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE refresh_tokens
        ADD CONSTRAINT refresh_tokens_expiry_valid CHECK (expires_at > created_at);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

-- Fast lookup when listing all refresh tokens for a user.
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id
ON refresh_tokens(user_id);

-- Supports scheduled cleanup of expired refresh tokens.
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires_at
ON refresh_tokens(expires_at);

-- Common lookup path for active refresh tokens by user.
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_active
ON refresh_tokens(user_id, revoked_at)
WHERE revoked_at IS NULL;
