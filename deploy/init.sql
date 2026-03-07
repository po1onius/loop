\c loop

create table users (
    user_id BIGSERIAL PRIMARY KEY,
    username TEXT NOT NULL,
    account TEXT UNIQUE NOT NULL,
    pwd TEXT NOT NULL,
    role TEXT NOT NULL
);

CREATE TABLE refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    device_id TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    ip_address INET,
    user_agent TEXT
);
-- 用户查询（查某用户所有token）
CREATE INDEX idx_refresh_tokens_user_id
ON refresh_tokens(user_id);

-- 过期清理
CREATE INDEX idx_refresh_tokens_expires_at
ON refresh_tokens(expires_at);

-- 常见查询：查某用户的有效token
CREATE INDEX idx_refresh_tokens_user_active
ON refresh_tokens(user_id, revoked_at)
WHERE revoked_at IS NULL;
