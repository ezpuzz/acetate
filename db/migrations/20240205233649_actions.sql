-- migrate:up
CREATE TABLE actions (
        action          TEXT    CHECK( action IN ('HIDE', 'WATCH') ) NOT NULL,
        identifier      TEXT    NOT NULL
);


-- migrate:down
DROP TABLE actions;
