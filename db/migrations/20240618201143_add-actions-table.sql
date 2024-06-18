CREATE TABLE actions (
        user_id         INT     REFERENCES users(user_id),
        action          TEXT    CHECK( action IN ('HIDE', 'WATCH') ) NOT NULL,
        identifier      TEXT    NOT NULL,
        PRIMARY KEY(action, identifier)
);
