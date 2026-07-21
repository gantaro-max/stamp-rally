ALTER TABLE players
ADD COLUMN stamp_card_token VARCHAR(36) NULL;
ALTER TABLE players
ADD UNIQUE KEY uq_players_stamp_card_token (stamp_card_token);