use sqlx::{MySqlPool, Row};

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


pub async fn find_by_uuid(
    pool: &MySqlPool,
    uuid: &str,
) -> Result<Option<(Vec<u8>, String)>, sqlx::Error> {
    let Some(row) = sqlx::query("SELECT data, mime_type FROM room_images WHERE uuid = ?")
        .bind(uuid)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };

    Ok(Some((row.try_get("data")?, row.try_get("mime_type")?)))
}

#[cfg(test)]
mod tests {
    #[sqlx::test]
    async fn finds_image_by_uuid(pool: sqlx::MySqlPool) {
        let data = b"jpeg-bytes";
        super::insert(&pool, "image-uuid", data, "image/jpeg").await.unwrap();

        let (found_data, mime_type) = super::find_by_uuid(&pool, "image-uuid").await.unwrap().unwrap();

        assert_eq!(found_data, data);
        assert_eq!(mime_type, "image/jpeg");
    }

    #[sqlx::test]
    async fn find_by_uuid_returns_none_for_missing_uuid(pool: sqlx::MySqlPool) {
        let image = super::find_by_uuid(&pool, "missing-uuid").await.unwrap();

        assert!(image.is_none());
    }
}
