-- 初始化当前开发阶段数据库结构。
-- 开发阶段不保留历史 schema 兼容迁移；表结构变更时直接更新本初始化脚本，
-- 本地环境通过重建数据库获得与代码一致的干净 schema。
-- 数据库层不使用外键，跨表引用关系由业务服务校验并通过普通索引保障查询效率。

CREATE TABLE users (
    user_id BIGSERIAL PRIMARY KEY,
    username TEXT NOT NULL,
    account TEXT NOT NULL,
    pwd TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    CONSTRAINT users_account_key UNIQUE (account),
    CONSTRAINT users_username_not_empty CHECK (length(btrim(username)) > 0),
    CONSTRAINT users_username_len CHECK (char_length(username) <= 50),
    CONSTRAINT users_account_not_empty CHECK (length(btrim(account)) > 0),
    CONSTRAINT users_account_len CHECK (char_length(account) <= 100),
    CONSTRAINT users_pwd_not_empty CHECK (length(pwd) > 0),
    CONSTRAINT users_role_not_empty CHECK (length(btrim(role)) > 0)
);

-- ==================== 仅用于开发测试，生产部署前请删除本区块 ====================
-- 三个账号用于验证“发布活动 -> 申请加入 -> 发布者审核”的完整流程。
-- 登录密码统一为：123456
-- pwd 已按后端 bcrypt DEFAULT_COST（当前为 12）生成，不能把明文密码直接写入 pwd 字段。
INSERT INTO users (username, account, pwd, role)
VALUES
    ('测试发布者', 't1@t.com', '$2b$12$.e.sQCAfkJYgg4EyRrIfvuaMBXWAp..xbZtshvuAThqTEvprB48Uq', 'organizer'),
    ('测试用户一', 't2@t.com', '$2b$12$.e.sQCAfkJYgg4EyRrIfvuaMBXWAp..xbZtshvuAThqTEvprB48Uq', 'user'),
    ('测试用户二', 't3@t.com', '$2b$12$.e.sQCAfkJYgg4EyRrIfvuaMBXWAp..xbZtshvuAThqTEvprB48Uq', 'user');
-- ==================== 仅用于开发测试，生产部署前请删除本区块 ====================

CREATE TABLE refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    token_hash TEXT NOT NULL,
    device_id TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ip_address INET,
    user_agent TEXT,
    CONSTRAINT refresh_tokens_token_hash_key UNIQUE (token_hash),
    CONSTRAINT refresh_tokens_token_hash_not_empty CHECK (length(btrim(token_hash)) > 0),
    CONSTRAINT refresh_tokens_expiry_valid CHECK (expires_at > created_at)
);

CREATE INDEX idx_refresh_tokens_user_id
ON refresh_tokens(user_id);

CREATE INDEX idx_refresh_tokens_expires_at
ON refresh_tokens(expires_at);

CREATE INDEX idx_refresh_tokens_user_active
ON refresh_tokens(user_id, revoked_at)
WHERE revoked_at IS NULL;

CREATE TABLE media_assets (
    asset_id TEXT PRIMARY KEY,
    owner_id BIGINT NOT NULL,
    storage_key TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    byte_size BIGINT NOT NULL,
    width INTEGER,
    height INTEGER,
    status TEXT NOT NULL DEFAULT 'pending',
    variants_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT media_assets_storage_key_key UNIQUE (storage_key),
    CONSTRAINT media_assets_asset_id_not_empty CHECK (length(btrim(asset_id)) > 0),
    CONSTRAINT media_assets_storage_key_not_empty CHECK (length(btrim(storage_key)) > 0),
    CONSTRAINT media_assets_mime_type_not_empty CHECK (length(btrim(mime_type)) > 0),
    CONSTRAINT media_assets_byte_size_positive CHECK (byte_size > 0),
    CONSTRAINT media_assets_width_positive CHECK (width IS NULL OR width > 0),
    CONSTRAINT media_assets_height_positive CHECK (height IS NULL OR height > 0)
);

CREATE INDEX idx_media_assets_owner_id
ON media_assets(owner_id);

CREATE INDEX idx_media_assets_status
ON media_assets(status);

CREATE TABLE events (
    event_id BIGSERIAL PRIMARY KEY,
    creator_id BIGINT NOT NULL,
    title TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'published',
    content_version INTEGER NOT NULL DEFAULT 1,
    content_doc JSONB NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    cover_asset_id TEXT,
    start_at TIMESTAMPTZ,
    end_at TIMESTAMPTZ,
    location_name TEXT,
    location_address TEXT,
    capacity INTEGER,
    requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at TIMESTAMPTZ,
    CONSTRAINT events_title_required_when_not_draft CHECK (status = 'draft' OR length(btrim(title)) > 0),
    CONSTRAINT events_title_len CHECK (char_length(title) <= 80),
    CONSTRAINT events_status_allowed CHECK (status IN ('draft', 'published', 'cancelled')),
    CONSTRAINT events_content_version_supported CHECK (content_version = 1),
    CONSTRAINT events_content_doc_blocks CHECK (jsonb_typeof(content_doc->'blocks') = 'array'),
    CONSTRAINT events_time_range_valid CHECK (end_at IS NULL OR start_at IS NULL OR end_at > start_at),
    CONSTRAINT events_capacity_positive CHECK (capacity IS NULL OR capacity > 0)
);

CREATE INDEX idx_events_status_created_at
ON events(status, created_at DESC);

CREATE INDEX idx_events_creator_id_created_at
ON events(creator_id, created_at DESC);

CREATE INDEX idx_events_creator_id_updated_at
ON events(creator_id, updated_at DESC);

CREATE INDEX idx_events_cover_asset_id
ON events(cover_asset_id)
WHERE cover_asset_id IS NOT NULL;

CREATE INDEX idx_events_start_at
ON events(start_at)
WHERE start_at IS NOT NULL;

CREATE INDEX idx_events_tags
ON events USING GIN(tags);

CREATE INDEX idx_events_content_doc
ON events USING GIN(content_doc jsonb_path_ops);

CREATE FUNCTION set_events_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_events_updated_at
BEFORE UPDATE ON events
FOR EACH ROW
EXECUTE FUNCTION set_events_updated_at();

-- 用户与活动的参与关系独立建表，并通过唯一约束保证同一用户在同一活动中只有一条记录。
-- 按项目约定不建立数据库外键；活动和用户是否存在、发布者是否有权审核均由业务层校验。
CREATE TABLE event_participations (
    event_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    status TEXT NOT NULL,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_at TIMESTAMPTZ,
    reviewed_by BIGINT,
    joined_at TIMESTAMPTZ,
    PRIMARY KEY (event_id, user_id),
    CONSTRAINT event_participations_status_allowed CHECK (status IN ('pending', 'joined', 'rejected')),
    CONSTRAINT event_participations_joined_at_consistent CHECK (
        (status = 'joined' AND joined_at IS NOT NULL) OR
        (status <> 'joined' AND joined_at IS NULL)
    ),
    CONSTRAINT event_participations_review_consistent CHECK (
        (reviewed_at IS NULL AND reviewed_by IS NULL) OR
        (reviewed_at IS NOT NULL AND reviewed_by IS NOT NULL)
    )
);

CREATE INDEX idx_event_participations_user_status
ON event_participations(user_id, status);

CREATE INDEX idx_event_participations_event_status_requested_at
ON event_participations(event_id, status, requested_at);
