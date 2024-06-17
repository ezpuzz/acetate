CREATE TABLE users (
    user_id INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    discogs_oauth_token VARCHAR(128) NOT NULL,
    discogs_oauth_token_secret VARCHAR(128) NOT NULL,
    discogs_user_id INT NOT NULL UNIQUE
);

CREATE TABLE actions (
        user_id         INT     REFERENCES users(user_id),
        action          TEXT    CHECK( action IN ('HIDE', 'WATCH') ) NOT NULL,
        identifier      TEXT    NOT NULL,
        UNIQUE(action, identifier)
);
