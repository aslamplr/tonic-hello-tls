-- Your SQL goes here
CREATE TABLE IF NOT EXISTS messages (
  id SERIAL PRIMARY KEY,
  message TEXT,
  updated INTEGER
);
