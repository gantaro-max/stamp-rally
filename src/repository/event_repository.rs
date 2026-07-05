#[cfg(test)]
mod tests {
    use super::count;
    use crate::services::auth_service::{seed_admin_event_if_empty, verify_password};

    #[sqlx::test]
    async fn seeds_initial_event_when_events_table_is_empty(pool: sqlx::MySqlPool) {
        seed_admin_event_if_empty(&pool, "initial-secret", "Stamp Rally")
            .await
            .unwrap();

        assert_eq!(count(&pool).await.unwrap(), 1);

        let event = super::find_singleton(&pool).await.unwrap().unwrap();
        assert_eq!(event.event_name, "Stamp Rally");
        assert_ne!(event.admin_pass_hash, "initial-secret");
        assert!(verify_password("initial-secret", &event.admin_pass_hash));
        assert!(!event.is_team_mode);
        assert!(!event.require_answer_check);
    }
}
