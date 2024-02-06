-- migrate:up
CREATE TABLE actions (
        action          TEXT    CHECK( action IN ('HIDE', 'WATCH') ) NOT NULL,
        identifier      TEXT    NOT NULL,
        UNIQUE(action, identifier)
);


-- migrate:down
DROP TABLE actions;
