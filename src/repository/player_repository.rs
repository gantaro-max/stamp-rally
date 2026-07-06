#[cfg(test)]
mod tests {
    async fn seed_event(pool: &sqlx::MySqlPool) -> i32 {
        crate::services::auth_service::seed_admin_event_if_empty(
            pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        crate::repository::event_repository::find_singleton(pool)
            .await
            .unwrap()
            .unwrap()
            .id
    }

    async fn seed_room(pool: &sqlx::MySqlPool, event_id: i32) -> i32 {
        crate::repository::room_repository::insert(
            pool,
            event_id,
            "Room",
            "Quest",
            None,
            None,
            None,
            "qr-player-room",
        )
        .await
        .unwrap()
    }

    #[sqlx::test]
    async fn inserts_and_finds_player_by_line_user_and_event(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;

        let player_id = super::insert(&pool, "line-user-1", event_id, "Alice").await.unwrap();
        let player = super::find_by_line_user_and_event(&pool, "line-user-1", event_id).await.unwrap().unwrap();

        assert_eq!(player.id, player_id);
        assert_eq!(player.line_user_id, "line-user-1");
        assert_eq!(player.event_id, event_id);
        assert_eq!(player.player_name, "Alice");
        assert_eq!(player.current_room_id, None);
        assert!(!player.answer_verified);
        assert!(player.finished_at.is_none());
    }

    #[sqlx::test]
    async fn update_current_room_sets_room_and_resets_answer_verified(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room_id = seed_room(&pool, event_id).await;
        let player_id = super::insert(&pool, "line-user-2", event_id, "Alice").await.unwrap();
        super::set_answer_verified(&pool, player_id, true).await.unwrap();

        super::update_current_room(&pool, player_id, room_id).await.unwrap();
        let player = super::find_by_line_user_and_event(&pool, "line-user-2", event_id).await.unwrap().unwrap();

        assert_eq!(player.current_room_id, Some(room_id));
        assert!(!player.answer_verified);
    }

    #[sqlx::test]
    async fn set_answer_verified_updates_flag(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let player_id = super::insert(&pool, "line-user-3", event_id, "Alice").await.unwrap();

        super::set_answer_verified(&pool, player_id, true).await.unwrap();
        let player = super::find_by_line_user_and_event(&pool, "line-user-3", event_id).await.unwrap().unwrap();

        assert!(player.answer_verified);
    }

    #[sqlx::test]
    async fn delete_by_line_user_and_event_deletes_player(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        super::insert(&pool, "line-user-4", event_id, "Alice").await.unwrap();

        super::delete_by_line_user_and_event(&pool, "line-user-4", event_id).await.unwrap();

        assert!(super::find_by_line_user_and_event(&pool, "line-user-4", event_id).await.unwrap().is_none());
    }

    #[sqlx::test]
    async fn delete_by_line_user_and_event_cascades_visited_rooms(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room_id = seed_room(&pool, event_id).await;
        let player_id = super::insert(&pool, "line-user-5", event_id, "Alice").await.unwrap();
        sqlx::query("INSERT INTO visited_rooms (player_id, room_id, visited_at) VALUES (?, ?, NOW())")
            .bind(player_id)
            .bind(room_id)
            .execute(&pool)
            .await
            .unwrap();

        super::delete_by_line_user_and_event(&pool, "line-user-5", event_id).await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM visited_rooms WHERE player_id = ?")
            .bind(player_id)
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(count, 0);
    }
}
