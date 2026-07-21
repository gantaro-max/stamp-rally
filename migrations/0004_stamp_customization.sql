ALTER TABLE rooms
    ADD COLUMN stamp_label VARCHAR(4) NULL;

ALTER TABLE rooms
    ADD COLUMN stamp_image_id INT NULL;

ALTER TABLE rooms
    ADD KEY idx_rooms_stamp_image_id (stamp_image_id);

ALTER TABLE rooms
    ADD CONSTRAINT fk_rooms_stamp_image_id
        FOREIGN KEY (stamp_image_id) REFERENCES room_images (id)
        ON DELETE SET NULL;

ALTER TABLE events
    ADD COLUMN stamp_card_background_image_id INT NULL;

ALTER TABLE events
    ADD KEY idx_events_stamp_card_background_image_id (stamp_card_background_image_id);

ALTER TABLE events
    ADD CONSTRAINT fk_events_stamp_card_background_image_id
        FOREIGN KEY (stamp_card_background_image_id) REFERENCES room_images (id)
        ON DELETE SET NULL;
