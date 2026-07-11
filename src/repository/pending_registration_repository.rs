use sqlx::{MySqlPool, Row};

pub async fn exists(
    pool: &MySqlPool,
    line_user_id: &str,
    event_id: i32,
) -> Result<bool, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT COUNT(*) AS count
        FROM pending_registrations
        WHERE line_user_id = ? AND event_id = ?
        "#,
    )
    .bind(line_user_id)
    .bind(event_id)
    .fetch_one(pool)
    .await?;

    Ok(row.try_get::<i64, _>("count")? > 0)
}

pub async fn insert(
    pool: &MySqlPool,
    line_user_id: &str,
    event_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO pending_registrations (line_user_id, event_id, created_at)
        VALUES (?, ?, NOW())
        ON DUPLICATE KEY UPDATE created_at = VALUES(created_at)
        "#,
    )
    .bind(line_user_id)
    .bind(event_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete(
    pool: &MySqlPool,
    line_user_id: &str,
    event_id: i32,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM pending_registrations
        WHERE line_user_id = ? AND event_id = ?
        "#,
    )
    .bind(line_user_id)
    .bind(event_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
