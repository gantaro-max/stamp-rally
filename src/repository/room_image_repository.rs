use sqlx::MySqlPool;

pub async fn insert(
    pool: &MySqlPool,
    uuid: &str,
    data: &[u8],
    mime_type: &str,
) -> Result<i32, sqlx::Error> {
    let result = sqlx::query(
        r#"
        INSERT INTO room_images (uuid, data, mime_type)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(uuid)
    .bind(data)
    .bind(mime_type)
    .execute(pool)
    .await?;

    Ok(result.last_insert_id() as i32)
}

pub async fn delete(pool: &MySqlPool, id: i32) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM room_images WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}
