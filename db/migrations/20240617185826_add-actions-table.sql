CREATE TABLE users (
    user_id INT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    discogs_access_token VARCHAR(2048),
    discogs_refresh_token VARCHAR(512)
);

CREATE TABLE actions (
        user_id         INT     REFERENCES users(user_id),
        action          TEXT    CHECK( action IN ('HIDE', 'WATCH') ) NOT NULL,
        identifier      TEXT    NOT NULL,
        UNIQUE(action, identifier)
);
