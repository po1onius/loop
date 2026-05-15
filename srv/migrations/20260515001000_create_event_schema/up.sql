-- Core event schema.
-- Event rich content is stored as versioned JSONB blocks while searchable
-- attributes stay in typed columns for efficient listing and filtering.

CREATE TABLE IF NOT EXISTS media_assets (
    asset_id TEXT PRIMARY KEY,
    owner_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    storage_key TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    byte_size BIGINT NOT NULL,
    width INTEGER,
    height INTEGER,
    status TEXT NOT NULL DEFAULT 'pending',
    variants_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

DO $$
BEGIN
    ALTER TABLE media_assets
        ADD CONSTRAINT media_assets_storage_key_key UNIQUE (storage_key);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE media_assets
        ADD CONSTRAINT media_assets_asset_id_not_empty CHECK (length(btrim(asset_id)) > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE media_assets
        ADD CONSTRAINT media_assets_storage_key_not_empty CHECK (length(btrim(storage_key)) > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE media_assets
        ADD CONSTRAINT media_assets_mime_type_not_empty CHECK (length(btrim(mime_type)) > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE media_assets
        ADD CONSTRAINT media_assets_byte_size_positive CHECK (byte_size > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE media_assets
        ADD CONSTRAINT media_assets_width_positive CHECK (width IS NULL OR width > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE media_assets
        ADD CONSTRAINT media_assets_height_positive CHECK (height IS NULL OR height > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

CREATE INDEX IF NOT EXISTS idx_media_assets_owner_id
ON media_assets(owner_id);

CREATE INDEX IF NOT EXISTS idx_media_assets_status
ON media_assets(status);

CREATE TABLE IF NOT EXISTS events (
    event_id BIGSERIAL PRIMARY KEY,
    creator_id BIGINT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'published',
    content_doc JSONB NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    cover_asset_id TEXT REFERENCES media_assets(asset_id) ON DELETE SET NULL,
    start_at TIMESTAMPTZ,
    end_at TIMESTAMPTZ,
    location_name TEXT,
    location_address TEXT,
    capacity INTEGER,
    tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at TIMESTAMPTZ
);

DO $$
BEGIN
    ALTER TABLE events
        ADD CONSTRAINT events_title_not_empty CHECK (length(btrim(title)) > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE events
        ADD CONSTRAINT events_title_len CHECK (char_length(title) <= 80);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE events
        ADD CONSTRAINT events_status_allowed CHECK (status IN ('draft', 'published', 'cancelled'));
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE events
        ADD CONSTRAINT events_content_doc_schema CHECK ((content_doc->>'schema_version') IN ('1', '2'));
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE events
        ADD CONSTRAINT events_time_range_valid CHECK (end_at IS NULL OR start_at IS NULL OR end_at > start_at);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE events
        ADD CONSTRAINT events_capacity_positive CHECK (capacity IS NULL OR capacity > 0);
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

CREATE INDEX IF NOT EXISTS idx_events_status_created_at
ON events(status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_events_creator_id_created_at
ON events(creator_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_events_start_at
ON events(start_at)
WHERE start_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_events_tags
ON events USING GIN(tags);

CREATE INDEX IF NOT EXISTS idx_events_content_doc
ON events USING GIN(content_doc jsonb_path_ops);

CREATE OR REPLACE FUNCTION set_events_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_events_updated_at ON events;
CREATE TRIGGER trg_events_updated_at
BEFORE UPDATE ON events
FOR EACH ROW
EXECUTE FUNCTION set_events_updated_at();
