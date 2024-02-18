DROP TABLE IF EXISTS ygo_quiz;
DROP TABLE IF EXISTS mtg_quiz;

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
