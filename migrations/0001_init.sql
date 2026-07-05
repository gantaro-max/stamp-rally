CREATE TABLE events (
    id INT AUTO_INCREMENT PRIMARY KEY,
    event_name VARCHAR(255) NOT NULL,
    admin_pass_hash VARCHAR(255) NOT NULL,
    is_team_mode BOOLEAN NOT NULL DEFAULT FALSE,
    require_answer_check BOOLEAN NOT NULL DEFAULT FALSE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE room_images (
    id INT AUTO_INCREMENT PRIMARY KEY,
    uuid VARCHAR(36) NOT NULL,
    data LONGBLOB NOT NULL,
    mime_type VARCHAR(255) NOT NULL,
    UNIQUE KEY uq_room_images_uuid (uuid)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE rooms (
    id INT AUTO_INCREMENT PRIMARY KEY,
    event_id INT NOT NULL,
    room_name VARCHAR(255) NOT NULL,
    quest_text TEXT NOT NULL,
    answer VARCHAR(255) NULL,
    hint_msg VARCHAR(255) NULL,
    image_id INT NULL,
    qr_uuid VARCHAR(36) NOT NULL,
    UNIQUE KEY uq_rooms_qr_uuid (qr_uuid),
    KEY idx_rooms_event_id (event_id),
    KEY idx_rooms_image_id (image_id),
    CONSTRAINT fk_rooms_event_id
        FOREIGN KEY (event_id) REFERENCES events (id)
        ON DELETE CASCADE,
    CONSTRAINT fk_rooms_image_id
        FOREIGN KEY (image_id) REFERENCES room_images (id)
        ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE players (
    id INT AUTO_INCREMENT PRIMARY KEY,
    line_user_id VARCHAR(255) NOT NULL,
    event_id INT NOT NULL,
    player_name VARCHAR(255) NOT NULL,
    current_room_id INT NULL,
    answer_verified BOOLEAN NOT NULL DEFAULT FALSE,
    started_at DATETIME NOT NULL,
    finished_at DATETIME NULL,
    UNIQUE KEY uq_players_line_user_event (line_user_id, event_id),
    KEY idx_players_event_id (event_id),
    KEY idx_players_current_room_id (current_room_id),
    CONSTRAINT fk_players_event_id
        FOREIGN KEY (event_id) REFERENCES events (id)
        ON DELETE CASCADE,
    CONSTRAINT fk_players_current_room_id
        FOREIGN KEY (current_room_id) REFERENCES rooms (id)
        ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE visited_rooms (
    player_id INT NOT NULL,
    room_id INT NOT NULL,
    visited_at DATETIME NOT NULL,
    PRIMARY KEY (player_id, room_id),
    KEY idx_visited_rooms_room_id (room_id),
    CONSTRAINT fk_visited_rooms_player_id
        FOREIGN KEY (player_id) REFERENCES players (id)
        ON DELETE CASCADE,
    CONSTRAINT fk_visited_rooms_room_id
        FOREIGN KEY (room_id) REFERENCES rooms (id)
        ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
