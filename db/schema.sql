CREATE TABLE IF NOT EXISTS "schema_migrations" (version varchar(128) primary key);
CREATE TABLE actions (
        action          TEXT    CHECK( action IN ('HIDE', 'WATCH') ) NOT NULL,
        identifier      TEXT    NOT NULL,
        UNIQUE(action, identifier)
);
-- Dbmate schema migrations
INSERT INTO "schema_migrations" (version) VALUES
  ('20240205233649');
