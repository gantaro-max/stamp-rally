use sqlx::{MySqlPool, Row};

#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
pub struct Event {
    pub id: i32,
    pub event_name: String,
    pub admin_pass_hash: String,
    pub is_team_mode: bool,
    pub require_answer_check: bool,
}

pub async fn count(pool: &MySqlPool) -> Result<i64, sqlx::Error> {
    let row = sqlx::query("SELECT COUNT(*) AS count FROM events")
        .fetch_one(pool)
        .await?;

    row.try_get("count")
}

pub async fn insert_initial(
    pool: &MySqlPool,
    event_name: &str,
    admin_pass_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO events (event_name, admin_pass_hash, is_team_mode, require_answer_check)
        VALUES (?, ?, FALSE, FALSE)
        "#,
    )
    .bind(event_name)
    .bind(admin_pass_hash)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn find_singleton(pool: &MySqlPool) -> Result<Option<Event>, sqlx::Error> {
    let Some(row) = sqlx::query(
        r#"
        SELECT id, event_name, admin_pass_hash, is_team_mode, require_answer_check
        FROM events
        ORDER BY id
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(Event {
        id: row.try_get("id")?,
        event_name: row.try_get("event_name")?,
        admin_pass_hash: row.try_get("admin_pass_hash")?,
        is_team_mode: row.try_get::<i8, _>("is_team_mode")? != 0,
        require_answer_check: row.try_get::<i8, _>("require_answer_check")? != 0,
    }))
}

pub async fn update_settings(
    pool: &MySqlPool,
    id: i32,
    is_team_mode: bool,
    require_answer_check: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE events SET is_team_mode = ?, require_answer_check = ? WHERE id = ?")
        .bind(is_team_mode)
        .bind(require_answer_check)
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

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

    #[sqlx::test]
    async fn does_not_seed_when_event_already_exists(pool: sqlx::MySqlPool) {
        seed_admin_event_if_empty(&pool, "first-secret", "First Event")
            .await
            .unwrap();
        let first = super::find_singleton(&pool).await.unwrap().unwrap();

        seed_admin_event_if_empty(&pool, "second-secret", "Second Event")
            .await
            .unwrap();

        assert_eq!(count(&pool).await.unwrap(), 1);
        let current = super::find_singleton(&pool).await.unwrap().unwrap();
        assert_eq!(current.id, first.id);
        assert_eq!(current.event_name, "First Event");
        assert!(verify_password("first-secret", &current.admin_pass_hash));
        assert!(!verify_password("second-secret", &current.admin_pass_hash));
    }

    #[sqlx::test]
    async fn updates_settings_both_directions(pool: sqlx::MySqlPool) {
        seed_admin_event_if_empty(&pool, "admin-secret", "Stamp Rally")
            .await
            .unwrap();
        let event = super::find_singleton(&pool).await.unwrap().unwrap();

        super::update_settings(&pool, event.id, true, true)
            .await
            .unwrap();
        let updated = super::find_singleton(&pool).await.unwrap().unwrap();
        assert!(updated.is_team_mode);
        assert!(updated.require_answer_check);

        super::update_settings(&pool, event.id, false, false)
            .await
            .unwrap();
        let updated = super::find_singleton(&pool).await.unwrap().unwrap();
        assert!(!updated.is_team_mode);
        assert!(!updated.require_answer_check);
    }
}
