DROP TABLE IF EXISTS ygo_quiz;
DROP TABLE IF EXISTS mtg_quiz;
DROP TABLE IF EXISTS quiz;
DROP TYPE IF EXISTS quiz_type;

CREATE TYPE quiz_type AS ENUM ('ygo', 'mtg');

CREATE TABLE ygo_quiz (
  user_id BIGINT PRIMARY KEY,
  konami_id BIGINT NOT NULL,
  card_name TEXT NOT NULL,
  card_name_ruby TEXT NOT NULL,
  card_text TEXT NOT NULL
);

CREATE TABLE mtg_quiz (
  user_id BIGINT PRIMARY KEY,
  scryfall_uri TEXT NOT NULL,
  card_name TEXT NOT NULL,
  english_name TEXT NOT NULL,
  card_text TEXT NOT NULL
);

CREATE TABLE quiz (
  user_id BIGINT PRIMARY KEY,
  quiz_type quiz_type
);
