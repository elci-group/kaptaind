CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email TEXT NOT NULL
);

CREATE TABLE orders (
    id SERIAL PRIMARY KEY,
    user_id INTEGER REFERENCES users (id)
);

CREATE INDEX idx_orders_user ON orders (user_id);
