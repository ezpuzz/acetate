-- Add migration script here
ALTER TABLE actions ALTER COLUMN identifier TYPE integer USING (identifier::integer);
