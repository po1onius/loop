ALTER TABLE events
    DROP CONSTRAINT IF EXISTS events_content_doc_schema;

ALTER TABLE events
    ADD CONSTRAINT events_content_doc_schema
    CHECK ((content_doc->>'schema_version') = '1');
