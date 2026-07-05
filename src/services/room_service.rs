#[cfg(test)]
mod tests {
    use super::{CreateRoomInput, RoomError};

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

    fn input(name: String) -> CreateRoomInput {
        CreateRoomInput {
            room_name: name,
            quest_text: "Quest".to_string(),
            answer: None,
            hint_msg: None,
            image_bytes: None,
        }
    }

    #[sqlx::test]
    async fn create_rejects_when_room_limit_reached(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        for index in 0..15 {
            crate::repository::room_repository::insert(
                &pool,
                event_id,
                &format!("Room {index}"),
                "Quest",
                None,
                None,
                None,
                &format!("qr-limit-{index}"),
            )
            .await
            .unwrap();
        }

        let err = super::create(&pool, event_id, input("Overflow".to_string()))
            .await
            .unwrap_err();

        assert!(matches!(err, RoomError::MaxRoomsReached));
        assert_eq!(crate::repository::room_repository::count(&pool, event_id).await.unwrap(), 15);
    }
}
