CREATE OR REPLACE VIEW active_users AS
SELECT * FROM users WHERE active;

CREATE VIEW order_totals AS
SELECT user_id, sum(total) FROM orders GROUP BY user_id;
