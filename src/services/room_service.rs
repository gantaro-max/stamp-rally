use sqlx::MySqlPool;
use uuid::Uuid;

use crate::{
    repository::{event_repository, room_image_repository, room_repository},
    services::image_service,
};

pub const MAX_ROOMS: i64 = 15;

#[derive(Debug)]
pub struct CreateRoomInput {
    pub room_name: String,
    pub quest_text: String,
    pub answer: Option<String>,
    pub hint_msg: Option<String>,
    pub image_bytes: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct UpdateRoomInput {
    pub room_name: String,
    pub quest_text: String,
    pub answer: Option<String>,
    pub hint_msg: Option<String>,
    pub image_bytes: Option<Vec<u8>>,
}

#[derive(Debug)]
pub enum RoomError {
    MaxRoomsReached,
    AnswerRequired,
    NotFound,
    Image(image_service::ImageError),
    Database(sqlx::Error),
}

impl From<sqlx::Error> for RoomError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err)
    }
}

impl std::fmt::Display for RoomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MaxRoomsReached => f.write_str("room limit reached"),
            Self::AnswerRequired => f.write_str("answer required"),
            Self::NotFound => f.write_str("room not found"),
            Self::Image(err) => write!(f, "image error: {err:?}"),
            Self::Database(err) => write!(f, "database error: {err}"),
        }
    }
}

impl std::error::Error for RoomError {}

pub async fn create(
    pool: &MySqlPool,
    event_id: i32,
    input: CreateRoomInput,
) -> Result<i32, RoomError> {
    if room_repository::count(pool, event_id).await? >= MAX_ROOMS {
        return Err(RoomError::MaxRoomsReached);
    }

    let require_answer_check = event_repository::find_singleton(pool)
        .await?
        .map(|event| event.require_answer_check)
        .unwrap_or(false);
    let answer = input.answer.as_deref();
    let hint_msg = input.hint_msg.as_deref();

    if require_answer_check && answer.is_none_or(|value| value.trim().is_empty()) {
        return Err(RoomError::AnswerRequired);
    }
    let (answer, hint_msg) = if require_answer_check {
        (answer, hint_msg)
    } else {
        (None, None)
    };

    let image_id = if let Some(bytes) = input.image_bytes {
        let processed = image_service::process_upload(&bytes).map_err(RoomError::Image)?;
        Some(
            room_image_repository::insert(
                pool,
                &Uuid::new_v4().to_string(),
                &processed,
                "image/jpeg",
            )
            .await?,
        )
    } else {
        None
    };

    room_repository::insert(
        pool,
        event_id,
        &input.room_name,
        &input.quest_text,
        answer,
        hint_msg,
        image_id,
        &Uuid::new_v4().to_string(),
    )
    .await
    .map_err(RoomError::Database)
}

pub async fn update(pool: &MySqlPool, id: i32, input: UpdateRoomInput) -> Result<(), RoomError> {
    let Some(existing) = room_repository::find_by_id(pool, id).await? else {
        return Err(RoomError::NotFound);
    };
    let require_answer_check = event_repository::find_singleton(pool)
        .await?
        .map(|event| event.require_answer_check)
        .unwrap_or(false);
    let answer = input.answer.as_deref();
    let hint_msg = input.hint_msg.as_deref();

    if require_answer_check && answer.is_none_or(|value| value.trim().is_empty()) {
        return Err(RoomError::AnswerRequired);
    }
    let (answer, hint_msg) = if require_answer_check {
        (answer, hint_msg)
    } else {
        (None, None)
    };

    let image_id = if let Some(bytes) = input.image_bytes {
        let processed = image_service::process_upload(&bytes).map_err(RoomError::Image)?;
        if let Some(old_image_id) = existing.image_id {
            room_image_repository::delete(pool, old_image_id).await?;
        }
        Some(
            room_image_repository::insert(
                pool,
                &Uuid::new_v4().to_string(),
                &processed,
                "image/jpeg",
            )
            .await?,
        )
    } else {
        existing.image_id
    };

    room_repository::update(
        pool,
        id,
        &input.room_name,
        &input.quest_text,
        answer,
        hint_msg,
        image_id,
    )
    .await?;

    Ok(())
}

pub async fn delete(pool: &MySqlPool, id: i32) -> Result<(), RoomError> {
    let Some(existing) = room_repository::find_by_id(pool, id).await? else {
        return Err(RoomError::NotFound);
    };

    if let Some(image_id) = existing.image_id {
        room_image_repository::delete(pool, image_id).await?;
    }
    room_repository::delete(pool, id).await?;

    Ok(())
}

pub async fn list(
    pool: &MySqlPool,
    event_id: i32,
) -> Result<Vec<room_repository::Room>, RoomError> {
    room_repository::find_all(pool, event_id)
        .await
        .map_err(RoomError::Database)
}

pub async fn get(pool: &MySqlPool, id: i32) -> Result<Option<room_repository::Room>, RoomError> {
    room_repository::find_by_id(pool, id)
        .await
        .map_err(RoomError::Database)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

    use super::{CreateRoomInput, RoomError};

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

    fn input(name: String) -> CreateRoomInput {
        CreateRoomInput {
            room_name: name,
            quest_text: "Quest".to_string(),
            answer: None,
            hint_msg: None,
            image_bytes: None,
        }
    }

    fn png_bytes() -> Vec<u8> {
        let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(32, 32, Rgb([10, 20, 30])));
        let mut output = Cursor::new(Vec::new());
        image.write_to(&mut output, ImageFormat::Png).unwrap();
        output.into_inner()
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
        assert_eq!(
            crate::repository::room_repository::count(&pool, event_id)
                .await
                .unwrap(),
            15
        );
    }

    #[sqlx::test]
    async fn create_requires_answer_when_answer_check_enabled(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        sqlx::query("UPDATE events SET require_answer_check = TRUE WHERE id = ?")
            .bind(event_id)
            .execute(&pool)
            .await
            .unwrap();

        let err = super::create(&pool, event_id, input("No Answer".to_string()))
            .await
            .unwrap_err();

        assert!(matches!(err, RoomError::AnswerRequired));
        assert_eq!(
            crate::repository::room_repository::count(&pool, event_id)
                .await
                .unwrap(),
            0
        );
    }

    #[sqlx::test]
    async fn create_ignores_answer_fields_when_answer_check_disabled(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room_id = super::create(
            &pool,
            event_id,
            CreateRoomInput {
                room_name: "Room".to_string(),
                quest_text: "Quest".to_string(),
                answer: Some("submitted".to_string()),
                hint_msg: Some("hint".to_string()),
                image_bytes: None,
            },
        )
        .await
        .unwrap();

        let room = crate::repository::room_repository::find_by_id(&pool, room_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(room.answer, None);
        assert_eq!(room.hint_msg, None);
    }

    #[sqlx::test]
    async fn create_with_image_stores_room_image(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;

        let room_id = super::create(
            &pool,
            event_id,
            CreateRoomInput {
                room_name: "Image Room".to_string(),
                quest_text: "Quest".to_string(),
                answer: None,
                hint_msg: None,
                image_bytes: Some(png_bytes()),
            },
        )
        .await
        .unwrap();

        let room = crate::repository::room_repository::find_by_id(&pool, room_id)
            .await
            .unwrap()
            .unwrap();
        let image_id = room.image_id.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_images WHERE id = ?")
            .bind(image_id)
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(count, 1);
    }

    #[sqlx::test]
    async fn current_event_returns_singleton_event(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;

        let event = super::current_event(&pool).await.unwrap();

        assert_eq!(event.id, event_id);
        assert_eq!(event.event_name, "Stamp Rally");
    }

    #[sqlx::test]
    async fn update_replaces_existing_image(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room_id = super::create(
            &pool,
            event_id,
            CreateRoomInput {
                room_name: "Image Room".to_string(),
                quest_text: "Quest".to_string(),
                answer: None,
                hint_msg: None,
                image_bytes: Some(png_bytes()),
            },
        )
        .await
        .unwrap();
        let old_image_id = crate::repository::room_repository::find_by_id(&pool, room_id)
            .await
            .unwrap()
            .unwrap()
            .image_id
            .unwrap();

        super::update(
            &pool,
            room_id,
            super::UpdateRoomInput {
                room_name: "Updated".to_string(),
                quest_text: "Updated Quest".to_string(),
                answer: None,
                hint_msg: None,
                image_bytes: Some(png_bytes()),
            },
        )
        .await
        .unwrap();

        let room = crate::repository::room_repository::find_by_id(&pool, room_id)
            .await
            .unwrap()
            .unwrap();
        let new_image_id = room.image_id.unwrap();
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
    async fn delete_removes_linked_image(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room_id = super::create(
            &pool,
            event_id,
            CreateRoomInput {
                room_name: "Image Room".to_string(),
                quest_text: "Quest".to_string(),
                answer: None,
                hint_msg: None,
                image_bytes: Some(png_bytes()),
            },
        )
        .await
        .unwrap();
        let image_id = crate::repository::room_repository::find_by_id(&pool, room_id)
            .await
            .unwrap()
            .unwrap()
            .image_id
            .unwrap();

        super::delete(&pool, room_id).await.unwrap();

        let image_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_images WHERE id = ?")
            .bind(image_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            crate::repository::room_repository::find_by_id(&pool, room_id)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(image_count, 0);
    }
}
