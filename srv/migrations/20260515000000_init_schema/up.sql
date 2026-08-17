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
    avatar_asset_id TEXT,
    CONSTRAINT users_account_key UNIQUE (account),
    CONSTRAINT users_username_not_empty CHECK (length(btrim(username)) > 0),
    CONSTRAINT users_username_len CHECK (char_length(username) <= 50),
    CONSTRAINT users_account_not_empty CHECK (length(btrim(account)) > 0),
    CONSTRAINT users_account_len CHECK (char_length(account) <= 100),
    CONSTRAINT users_pwd_not_empty CHECK (length(pwd) > 0),
    CONSTRAINT users_role_not_empty CHECK (length(btrim(role)) > 0)
);

-- 头像资源由业务层校验属于当前用户且已经上传完成。按项目约定不建立外键，
-- 普通索引用于后续资源引用分析和清理任务。
CREATE INDEX idx_users_avatar_asset_id
ON users(avatar_asset_id)
WHERE avatar_asset_id IS NOT NULL;

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
    -- 同一次登录后轮换出的 refresh token 共享 family_id；发现旧 token 重放时，
    -- 可一次撤销该登录会话当前仍活跃的 token。按项目约定不添加数据库外键。
    family_id UUID NOT NULL,
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

CREATE INDEX idx_refresh_tokens_family_active
ON refresh_tokens(family_id, revoked_at)
WHERE revoked_at IS NULL;

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

-- 社区板块由数据库维护，便于后续增加后台排序和停用能力。section_id 使用稳定的
-- 业务字符串，客户端路由和筛选条件不依赖展示名称。
CREATE TABLE community_sections (
    section_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT community_sections_id_not_empty CHECK (length(btrim(section_id)) > 0),
    CONSTRAINT community_sections_name_not_empty CHECK (length(btrim(name)) > 0),
    CONSTRAINT community_sections_status_allowed CHECK (status IN ('active', 'disabled'))
);

INSERT INTO community_sections (section_id, name, description, sort_order)
VALUES
    ('tech', '科技', 'Web3、AI、开发者交流与线下分享。', 10),
    ('food', '美食', '探店、烘焙、咖啡和城市餐桌活动。', 20),
    ('photo', '摄影', '约拍、器材经验和主题创作活动。', 30),
    ('outdoor', '户外', '徒步、骑行、飞盘和周末轻运动。', 40);

-- conversations 是帖子讨论与未来活动群聊共用的会话内核。subject_id 保存对应
-- 业务实体的 UUID 字符串；活动目前仍使用 BIGINT，因此统一用 TEXT 承载且不建外键。
CREATE TABLE conversations (
    conversation_id UUID PRIMARY KEY,
    kind TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    access_mode TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    title TEXT NOT NULL,
    last_seq BIGINT NOT NULL DEFAULT 0,
    message_count BIGINT NOT NULL DEFAULT 0,
    last_message_preview TEXT,
    last_message_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT conversations_subject_key UNIQUE (kind, subject_id),
    CONSTRAINT conversations_kind_allowed CHECK (kind IN ('post_thread', 'event_group')),
    CONSTRAINT conversations_access_mode_allowed CHECK (access_mode IN ('open', 'restricted')),
    CONSTRAINT conversations_status_allowed CHECK (status IN ('active', 'locked', 'hidden')),
    CONSTRAINT conversations_title_not_empty CHECK (length(btrim(title)) > 0),
    CONSTRAINT conversations_counters_nonnegative CHECK (last_seq >= 0 AND message_count >= 0)
);

CREATE INDEX idx_conversations_last_message_at
ON conversations(last_message_at DESC NULLS LAST, conversation_id DESC);

CREATE TABLE community_posts (
    post_id UUID PRIMARY KEY,
    author_id BIGINT NOT NULL,
    section_id TEXT NOT NULL,
    post_type TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    image_asset_ids TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    status TEXT NOT NULL DEFAULT 'published',
    discussion_conversation_id UUID NOT NULL,
    discussion_count BIGINT NOT NULL DEFAULT 0,
    interest_count BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_activity_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at TIMESTAMPTZ,
    CONSTRAINT community_posts_conversation_key UNIQUE (discussion_conversation_id),
    CONSTRAINT community_posts_type_allowed CHECK (post_type IN ('event_idea', 'event_discussion', 'general')),
    CONSTRAINT community_posts_status_allowed CHECK (status IN ('published', 'hidden', 'deleted')),
    CONSTRAINT community_posts_title_not_empty CHECK (length(btrim(title)) > 0),
    CONSTRAINT community_posts_title_len CHECK (char_length(title) <= 120),
    CONSTRAINT community_posts_body_not_empty CHECK (length(btrim(body)) > 0),
    CONSTRAINT community_posts_body_len CHECK (char_length(body) <= 10000),
    CONSTRAINT community_posts_image_count CHECK (cardinality(image_asset_ids) <= 9),
    CONSTRAINT community_posts_counters_nonnegative CHECK (discussion_count >= 0 AND interest_count >= 0)
);

CREATE INDEX idx_community_posts_section_created
ON community_posts(section_id, created_at DESC, post_id DESC)
WHERE status = 'published';

CREATE INDEX idx_community_posts_activity
ON community_posts(last_activity_at DESC, post_id DESC)
WHERE status = 'published';

CREATE INDEX idx_community_posts_author_created
ON community_posts(author_id, created_at DESC, post_id DESC)
WHERE status = 'published';

CREATE INDEX idx_community_posts_image_assets
ON community_posts USING GIN(image_asset_ids);

CREATE TABLE conversation_messages (
    message_id UUID PRIMARY KEY,
    conversation_id UUID NOT NULL,
    seq BIGINT NOT NULL,
    sender_id BIGINT NOT NULL,
    client_message_id UUID NOT NULL,
    message_type TEXT NOT NULL DEFAULT 'text',
    body TEXT NOT NULL DEFAULT '',
    image_asset_ids TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    quote_message_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    CONSTRAINT conversation_messages_seq_key UNIQUE (conversation_id, seq),
    CONSTRAINT conversation_messages_client_key UNIQUE (sender_id, client_message_id),
    CONSTRAINT conversation_messages_seq_positive CHECK (seq > 0),
    CONSTRAINT conversation_messages_type_allowed CHECK (message_type IN ('text', 'image', 'system')),
    CONSTRAINT conversation_messages_body_len CHECK (char_length(body) <= 2000),
    CONSTRAINT conversation_messages_image_count CHECK (cardinality(image_asset_ids) <= 4),
    CONSTRAINT conversation_messages_content_present CHECK (
        message_type = 'system' OR length(btrim(body)) > 0 OR cardinality(image_asset_ids) > 0
    )
);

CREATE INDEX idx_conversation_messages_timeline
ON conversation_messages(conversation_id, seq DESC);

CREATE INDEX idx_conversation_messages_sender_created
ON conversation_messages(sender_id, created_at DESC);

CREATE TABLE conversation_read_states (
    conversation_id UUID NOT NULL,
    user_id BIGINT NOT NULL,
    last_read_seq BIGINT NOT NULL DEFAULT 0,
    subscribed BOOLEAN NOT NULL DEFAULT FALSE,
    muted BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (conversation_id, user_id),
    CONSTRAINT conversation_read_states_seq_nonnegative CHECK (last_read_seq >= 0)
);

CREATE INDEX idx_conversation_read_states_user_subscribed
ON conversation_read_states(user_id, updated_at DESC)
WHERE subscribed = TRUE;

CREATE TABLE community_post_reactions (
    post_id UUID NOT NULL,
    user_id BIGINT NOT NULL,
    reaction_type TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (post_id, user_id, reaction_type),
    CONSTRAINT community_post_reactions_type_allowed CHECK (reaction_type IN ('interested'))
);

CREATE INDEX idx_community_post_reactions_user_created
ON community_post_reactions(user_id, created_at DESC);

CREATE TABLE community_post_event_links (
    post_id UUID NOT NULL,
    event_id BIGINT NOT NULL,
    relation_type TEXT NOT NULL,
    created_by BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (post_id, event_id, relation_type),
    CONSTRAINT community_post_event_links_relation_allowed CHECK (relation_type IN ('discusses', 'spawned_event'))
);

CREATE INDEX idx_community_post_event_links_event
ON community_post_event_links(event_id, relation_type);

CREATE TABLE content_reports (
    report_id UUID PRIMARY KEY,
    reporter_id BIGINT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_at TIMESTAMPTZ,
    reviewed_by BIGINT,
    CONSTRAINT content_reports_target_allowed CHECK (target_type IN ('community_post', 'conversation_message')),
    CONSTRAINT content_reports_reason_not_empty CHECK (length(btrim(reason)) > 0),
    CONSTRAINT content_reports_status_allowed CHECK (status IN ('pending', 'resolved', 'dismissed'))
);

CREATE INDEX idx_content_reports_status_created
ON content_reports(status, created_at ASC);
