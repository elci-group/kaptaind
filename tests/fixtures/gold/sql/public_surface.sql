CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email TEXT NOT NULL
);

CREATE OR REPLACE VIEW active_users AS
SELECT * FROM users;

CREATE UNIQUE INDEX idx_users_email ON users (email);

CREATE FUNCTION user_count() RETURNS integer AS $$
SELECT count(*) FROM users;
$$ LANGUAGE SQL;

DROP TABLE IF EXISTS legacy_sessions;
