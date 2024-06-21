-- Add migration script here
ALTER TABLE users ADD COLUMN username VARCHAR(255);
UPDATE users SET username='eepman' WHERE discogs_user_id = 433159;
UPDATE users SET username='ayching' WHERE discogs_user_id = 22319476;
UPDATE users SET username='djchanadian' WHERE discogs_user_id = 3631171;
ALTER TABLE users ALTER COLUMN username TYPE VARCHAR(255), alter column username set NOT NULL;