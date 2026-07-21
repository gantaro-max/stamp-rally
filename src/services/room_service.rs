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
    pub stamp_label: String,
    pub stamp_image_bytes: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct UpdateRoomInput {
    pub room_name: String,
    pub quest_text: String,
    pub answer: Option<String>,
    pub hint_msg: Option<String>,
    pub image_bytes: Option<Vec<u8>>,
    pub stamp_label: String,
    pub stamp_image_bytes: Option<Vec<u8>>,
}

#[derive(Debug)]
pub enum RoomError {
    MaxRoomsReached,
    AnswerRequired,
    StampLabelInvalid,
    NotFound,
    Image,
    Database,
}

impl From<sqlx::Error> for RoomError {
    fn from(_: sqlx::Error) -> Self {
        Self::Database
    }
}

pub async fn current_event(pool: &MySqlPool) -> Result<event_repository::Event, RoomError> {
    event_repository::find_singleton(pool)
        .await?
        .ok_or(RoomError::NotFound)
}

fn validate_stamp_label(stamp_label: &str) -> Result<(), RoomError> {
    let len = stamp_label.chars().count();
    if len == 0 || len > 4 {
        return Err(RoomError::StampLabelInvalid);
    }
    Ok(())
}

async fn insert_uploaded_image(
    pool: &MySqlPool,
    bytes: Option<Vec<u8>>,
) -> Result<Option<i32>, RoomError> {
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    let processed = image_service::process_upload(&bytes).map_err(|_| RoomError::Image)?;
    let image_id =
        room_image_repository::insert(pool, &Uuid::new_v4().to_string(), &processed, "image/jpeg")
            .await?;
    Ok(Some(image_id))
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

    validate_stamp_label(&input.stamp_label)?;

    if require_answer_check && answer.is_none_or(|value| value.trim().is_empty()) {
        return Err(RoomError::AnswerRequired);
    }
    let (answer, hint_msg) = if require_answer_check {
        (answer, hint_msg)
    } else {
        (None, None)
    };

    let image_id = insert_uploaded_image(pool, input.image_bytes).await?;
    let stamp_image_id = insert_uploaded_image(pool, input.stamp_image_bytes).await?;

    room_repository::insert(
        pool,
        event_id,
        &input.room_name,
        &input.quest_text,
        answer,
        hint_msg,
        image_id,
        Some(input.stamp_label.as_str()),
        stamp_image_id,
        &Uuid::new_v4().to_string(),
    )
    .await
    .map_err(|_| RoomError::Database)
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

    validate_stamp_label(&input.stamp_label)?;

    if require_answer_check && answer.is_none_or(|value| value.trim().is_empty()) {
        return Err(RoomError::AnswerRequired);
    }
    let (answer, hint_msg) = if require_answer_check {
        (answer, hint_msg)
    } else {
        (None, None)
    };

    let old_image_id = existing.image_id;
    let old_stamp_image_id = existing.stamp_image_id;
    let image_id = match insert_uploaded_image(pool, input.image_bytes).await? {
        Some(new_image_id) => Some(new_image_id),
        None => old_image_id,
    };
    let stamp_image_id = match insert_uploaded_image(pool, input.stamp_image_bytes).await? {
        Some(new_image_id) => Some(new_image_id),
        None => old_stamp_image_id,
    };

    room_repository::update(
        pool,
        id,
        &input.room_name,
        &input.quest_text,
        answer,
        hint_msg,
        image_id,
        Some(input.stamp_label.as_str()),
        stamp_image_id,
    )
    .await?;

    if image_id != old_image_id
        && let Some(old_image_id) = old_image_id
    {
        room_image_repository::delete(pool, old_image_id).await?;
    }
    if stamp_image_id != old_stamp_image_id
        && let Some(old_stamp_image_id) = old_stamp_image_id
    {
        room_image_repository::delete(pool, old_stamp_image_id).await?;
    }

    Ok(())
}

pub async fn delete(pool: &MySqlPool, id: i32) -> Result<(), RoomError> {
    let Some(existing) = room_repository::find_by_id(pool, id).await? else {
        return Err(RoomError::NotFound);
    };

    if let Some(image_id) = existing.image_id {
        room_image_repository::delete(pool, image_id).await?;
    }
    if let Some(stamp_image_id) = existing.stamp_image_id {
        room_image_repository::delete(pool, stamp_image_id).await?;
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
        .map_err(|_| RoomError::Database)
}

pub async fn get(pool: &MySqlPool, id: i32) -> Result<Option<room_repository::Room>, RoomError> {
    room_repository::find_by_id(pool, id)
        .await
        .map_err(|_| RoomError::Database)
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
            stamp_label: "印".to_string(),
            stamp_image_bytes: None,
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
                stamp_label: "印".to_string(),
                stamp_image_bytes: None,
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
                stamp_label: "印".to_string(),
                stamp_image_bytes: None,
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
                stamp_label: "印".to_string(),
                stamp_image_bytes: None,
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
                stamp_label: "印".to_string(),
                stamp_image_bytes: None,
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
                stamp_label: "印".to_string(),
                stamp_image_bytes: None,
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
    #[sqlx::test]
    async fn create_rejects_empty_stamp_label_without_creating_room(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let mut input = input("Room".to_string());
        input.stamp_label = String::new();

        let err = super::create(&pool, event_id, input).await.unwrap_err();

        assert!(matches!(err, RoomError::StampLabelInvalid));
        assert_eq!(
            crate::repository::room_repository::count(&pool, event_id)
                .await
                .unwrap(),
            0
        );
    }

    #[sqlx::test]
    async fn create_rejects_stamp_label_longer_than_four_chars(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let mut input = input("Room".to_string());
        input.stamp_label = "12345".to_string();

        let err = super::create(&pool, event_id, input).await.unwrap_err();

        assert!(matches!(err, RoomError::StampLabelInvalid));
    }

    #[sqlx::test]
    async fn create_accepts_valid_stamp_label(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let mut input = input("Room".to_string());
        input.stamp_label = "図書".to_string();

        let room_id = super::create(&pool, event_id, input).await.unwrap();

        let room = crate::repository::room_repository::find_by_id(&pool, room_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(room.stamp_label.as_deref(), Some("図書"));
    }

    #[sqlx::test]
    async fn create_with_stamp_image_stores_room_image(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let mut input = input("Stamp Image Room".to_string());
        input.stamp_image_bytes = Some(png_bytes());

        let room_id = super::create(&pool, event_id, input).await.unwrap();

        let room = crate::repository::room_repository::find_by_id(&pool, room_id)
            .await
            .unwrap()
            .unwrap();
        let stamp_image_id = room.stamp_image_id.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_images WHERE id = ?")
            .bind(stamp_image_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[sqlx::test]
    async fn update_replaces_existing_stamp_image(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let mut input = input("Stamp Image Room".to_string());
        input.stamp_image_bytes = Some(png_bytes());
        let room_id = super::create(&pool, event_id, input).await.unwrap();
        let old_stamp_image_id = crate::repository::room_repository::find_by_id(&pool, room_id)
            .await
            .unwrap()
            .unwrap()
            .stamp_image_id
            .unwrap();

        super::update(
            &pool,
            room_id,
            super::UpdateRoomInput {
                room_name: "Updated".to_string(),
                quest_text: "Updated Quest".to_string(),
                answer: None,
                hint_msg: None,
                image_bytes: None,
                stamp_label: "更新".to_string(),
                stamp_image_bytes: Some(png_bytes()),
            },
        )
        .await
        .unwrap();

        let room = crate::repository::room_repository::find_by_id(&pool, room_id)
            .await
            .unwrap()
            .unwrap();
        let new_stamp_image_id = room.stamp_image_id.unwrap();
        let old_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_images WHERE id = ?")
            .bind(old_stamp_image_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let new_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_images WHERE id = ?")
            .bind(new_stamp_image_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_ne!(new_stamp_image_id, old_stamp_image_id);
        assert_eq!(old_count, 0);
        assert_eq!(new_count, 1);
    }

    #[sqlx::test]
    async fn update_without_stamp_image_keeps_existing_stamp_image(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let mut input = input("Stamp Image Room".to_string());
        input.stamp_image_bytes = Some(png_bytes());
        let room_id = super::create(&pool, event_id, input).await.unwrap();
        let existing_stamp_image_id =
            crate::repository::room_repository::find_by_id(&pool, room_id)
                .await
                .unwrap()
                .unwrap()
                .stamp_image_id;

        super::update(
            &pool,
            room_id,
            super::UpdateRoomInput {
                room_name: "Updated".to_string(),
                quest_text: "Updated Quest".to_string(),
                answer: None,
                hint_msg: None,
                image_bytes: None,
                stamp_label: "更新".to_string(),
                stamp_image_bytes: None,
            },
        )
        .await
        .unwrap();

        let room = crate::repository::room_repository::find_by_id(&pool, room_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(room.stamp_image_id, existing_stamp_image_id);
    }

    #[sqlx::test]
    async fn delete_removes_linked_stamp_image(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let mut input = input("Stamp Image Room".to_string());
        input.stamp_image_bytes = Some(png_bytes());
        let room_id = super::create(&pool, event_id, input).await.unwrap();
        let stamp_image_id = crate::repository::room_repository::find_by_id(&pool, room_id)
            .await
            .unwrap()
            .unwrap()
            .stamp_image_id
            .unwrap();

        super::delete(&pool, room_id).await.unwrap();

        let image_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_images WHERE id = ?")
            .bind(stamp_image_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(image_count, 0);
    }
}
