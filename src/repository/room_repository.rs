#[cfg(test)]
mod tests {
    async fn seed_event(pool: &sqlx::MySqlPool) -> i32 {
        crate::services::auth_service::seed_admin_event_if_empty(pool, "admin-secret", "Stamp Rally")
            .await
            .unwrap();
        crate::repository::event_repository::find_singleton(pool)
            .await
            .unwrap()
            .unwrap()
            .id
    }

    #[sqlx::test]
    async fn inserts_and_finds_room_by_id(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;

        let room_id = super::insert(
            &pool,
            event_id,
            "Room A",
            "Find the red mark",
            Some("red"),
            Some("look up"),
            None,
            "qr-uuid-1",
        )
        .await
        .unwrap();

        let room = super::find_by_id(&pool, room_id).await.unwrap().unwrap();

        assert_eq!(room.id, room_id);
        assert_eq!(room.event_id, event_id);
        assert_eq!(room.room_name, "Room A");
        assert_eq!(room.quest_text, "Find the red mark");
        assert_eq!(room.answer.as_deref(), Some("red"));
        assert_eq!(room.hint_msg.as_deref(), Some("look up"));
        assert_eq!(room.image_id, None);
        assert_eq!(room.qr_uuid, "qr-uuid-1");
    }
}
