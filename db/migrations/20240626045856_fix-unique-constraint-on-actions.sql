-- Add migration script here
ALTER TABLE actions DROP CONSTRAINT actions_pkey;
ALTER TABLE actions ADD CONSTRAINT actions_pkey PRIMARY KEY (user_id, action, identifier);
