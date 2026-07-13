SELECT * FROM users WHERE active;

INSERT INTO users (email) VALUES ('a@example.com');

UPDATE users SET active = false WHERE id = 1;

DELETE FROM sessions WHERE id = 2;
