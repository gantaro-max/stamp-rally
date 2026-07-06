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

    #[sqlx::test]
    async fn current_returns_singleton_event(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;

        let event = super::current(&pool).await.unwrap();

        assert_eq!(event.id, event_id);
        assert_eq!(event.event_name, "Stamp Rally");
    }

    #[sqlx::test]
    async fn update_settings_updates_event_flags(pool: sqlx::MySqlPool) {
        seed_event(&pool).await;

        super::update_settings(
            &pool,
            super::SettingsInput {
                is_team_mode: true,
                require_answer_check: true,
            },
        )
        .await
        .unwrap();
        let event = crate::repository::event_repository::find_singleton(&pool)
            .await
            .unwrap()
            .unwrap();

        assert!(event.is_team_mode);
        assert!(event.require_answer_check);
    }
}
