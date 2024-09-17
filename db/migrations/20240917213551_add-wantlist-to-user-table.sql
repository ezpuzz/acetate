-- Add migration script here
ALTER TABLE users ADD COLUMN wantlist bytea;
