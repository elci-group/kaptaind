-- CREATE TABLE old_users (id int);

/*
CREATE TABLE legacy_sessions (
    id int
);
*/

CREATE TABLE sessions (
    id SERIAL PRIMARY KEY
);

CREATE VIEW session_count AS
SELECT 1;
