use sqlx::MySqlPool;
use uuid::Uuid;

use crate::repository::{event_repository, room_repository};

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
pub enum RoomError {
    MaxRoomsReached,
    AnswerRequired,
    NotFound,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for RoomError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err)
    }
}

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

    room_repository::insert(
        pool,
        event_id,
        &input.room_name,
        &input.quest_text,
        answer,
        hint_msg,
        None,
        &Uuid::new_v4().to_string(),
    )
    .await
    .map_err(RoomError::Database)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

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
        assert_eq!(crate::repository::room_repository::count(&pool, event_id).await.unwrap(), 15);
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
        assert_eq!(crate::repository::room_repository::count(&pool, event_id).await.unwrap(), 0);
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
}
