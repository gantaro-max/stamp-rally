use sqlx::MySqlPool;
use uuid::Uuid;

use crate::{
    repository::{event_repository, room_image_repository},
    services::image_service,
};

#[derive(Debug)]
pub enum EventError {
    NotFound,
    Image,
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
            Self::Image => f.write_str("image error"),
            Self::Database(err) => write!(f, "database error: {err}"),
        }
    }
}

impl std::error::Error for EventError {}

#[derive(Debug)]
pub struct SettingsInput {
    pub is_team_mode: bool,
    pub require_answer_check: bool,
    pub stamp_card_background_image_bytes: Option<Vec<u8>>,
}

pub async fn current(pool: &MySqlPool) -> Result<event_repository::Event, EventError> {
    event_repository::find_singleton(pool)
        .await?
        .ok_or(EventError::NotFound)
}

pub async fn update_settings(pool: &MySqlPool, input: SettingsInput) -> Result<(), EventError> {
    let event = current(pool).await?;
    let old_background_image_id = event.stamp_card_background_image_id;
    let background_image_id = match insert_uploaded_image(pool, input.stamp_card_background_image_bytes).await? {
        Some(new_image_id) => Some(new_image_id),
        None => old_background_image_id,
    };

    event_repository::update_settings(
        pool,
        event.id,
        input.is_team_mode,
        input.require_answer_check,
        background_image_id,
    )
    .await?;

    if background_image_id != old_background_image_id {
        if let Some(old_background_image_id) = old_background_image_id {
            room_image_repository::delete(pool, old_background_image_id).await?;
        }
    }

    Ok(())
}

async fn insert_uploaded_image(
    pool: &MySqlPool,
    bytes: Option<Vec<u8>>,
) -> Result<Option<i32>, EventError> {
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    let processed = image_service::process_upload(&bytes).map_err(|_| EventError::Image)?;
    let image_id = room_image_repository::insert(
        pool,
        &Uuid::new_v4().to_string(),
        &processed,
        "image/jpeg",
    )
    .await?;
    Ok(Some(image_id))
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
                stamp_card_background_image_bytes: None,
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
    use std::io::Cursor;

    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

    fn png_bytes() -> Vec<u8> {
        let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(32, 32, Rgb([10, 20, 30])));
        let mut output = Cursor::new(Vec::new());
        image.write_to(&mut output, ImageFormat::Png).unwrap();
        output.into_inner()
    }

    #[sqlx::test]
    async fn update_settings_with_background_image_stores_room_image(pool: sqlx::MySqlPool) {
        seed_event(&pool).await;

        super::update_settings(
            &pool,
            super::SettingsInput {
                is_team_mode: true,
                require_answer_check: false,
                stamp_card_background_image_bytes: Some(png_bytes()),
            },
        )
        .await
        .unwrap();

        let event = crate::repository::event_repository::find_singleton(&pool).await.unwrap().unwrap();
        let image_id = event.stamp_card_background_image_id.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_images WHERE id = ?")
            .bind(image_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[sqlx::test]
    async fn update_settings_replaces_existing_background_image(pool: sqlx::MySqlPool) {
        seed_event(&pool).await;
        super::update_settings(
            &pool,
            super::SettingsInput {
                is_team_mode: true,
                require_answer_check: false,
                stamp_card_background_image_bytes: Some(png_bytes()),
            },
        )
        .await
        .unwrap();
        let old_image_id = crate::repository::event_repository::find_singleton(&pool)
            .await
            .unwrap()
            .unwrap()
            .stamp_card_background_image_id
            .unwrap();

        super::update_settings(
            &pool,
            super::SettingsInput {
                is_team_mode: false,
                require_answer_check: true,
                stamp_card_background_image_bytes: Some(png_bytes()),
            },
        )
        .await
        .unwrap();

        let event = crate::repository::event_repository::find_singleton(&pool).await.unwrap().unwrap();
        let new_image_id = event.stamp_card_background_image_id.unwrap();
        let old_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_images WHERE id = ?")
            .bind(old_image_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let new_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_images WHERE id = ?")
            .bind(new_image_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_ne!(new_image_id, old_image_id);
        assert_eq!(old_count, 0);
        assert_eq!(new_count, 1);
    }

    #[sqlx::test]
    async fn update_settings_without_background_image_keeps_existing_background_image(pool: sqlx::MySqlPool) {
        seed_event(&pool).await;
        super::update_settings(
            &pool,
            super::SettingsInput {
                is_team_mode: true,
                require_answer_check: false,
                stamp_card_background_image_bytes: Some(png_bytes()),
            },
        )
        .await
        .unwrap();
        let existing_image_id = crate::repository::event_repository::find_singleton(&pool)
            .await
            .unwrap()
            .unwrap()
            .stamp_card_background_image_id;

        super::update_settings(
            &pool,
            super::SettingsInput {
                is_team_mode: false,
                require_answer_check: false,
                stamp_card_background_image_bytes: None,
            },
        )
        .await
        .unwrap();

        let event = crate::repository::event_repository::find_singleton(&pool).await.unwrap().unwrap();
        assert_eq!(event.stamp_card_background_image_id, existing_image_id);
    }

}
