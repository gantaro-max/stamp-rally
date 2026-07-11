CREATE TABLE pending_registrations (
    line_user_id VARCHAR(255) NOT NULL,
    event_id INT NOT NULL,
    created_at DATETIME NOT NULL,
    PRIMARY KEY (line_user_id, event_id),
    CONSTRAINT fk_pending_registrations_event_id
        FOREIGN KEY (event_id) REFERENCES events (id)
        ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
