use sqlx::MySqlPool;

use crate::repository::event_repository;

#[derive(Debug)]
pub enum EventError {
    NotFound,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for EventError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err)
    }
}

impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("event not found"),
            Self::Database(err) => write!(f, "database error: {err}"),
        }
    }
}

impl std::error::Error for EventError {}

#[derive(Debug, Clone, Copy)]
pub struct SettingsInput {
    pub is_team_mode: bool,
    pub require_answer_check: bool,
}

pub async fn current(pool: &MySqlPool) -> Result<event_repository::Event, EventError> {
    event_repository::find_singleton(pool)
        .await?
        .ok_or(EventError::NotFound)
}

pub async fn update_settings(pool: &MySqlPool, input: SettingsInput) -> Result<(), EventError> {
    let event = current(pool).await?;
    event_repository::update_settings(
        pool,
        event.id,
        input.is_team_mode,
        input.require_answer_check,
    )
    .await?;
    Ok(())
}

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
