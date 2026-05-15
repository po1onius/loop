-- Allow the richer block-editor document schema while keeping existing v1
-- activity content readable.

ALTER TABLE events
    DROP CONSTRAINT IF EXISTS events_content_doc_schema;

ALTER TABLE events
    ADD CONSTRAINT events_content_doc_schema
    CHECK ((content_doc->>'schema_version') IN ('1', '2'));
