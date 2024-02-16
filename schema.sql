DROP TABLE IF EXISTS ygo_quiz;

CREATE TABLE ygo_quiz (
  user_id BIGINT PRIMARY KEY,
  konami_id BIGINT NOT NULL,
  card_name TEXT NOT NULL,
  card_name_ruby TEXT NOT NULL,
  card_text TEXT NOT NULL
);
