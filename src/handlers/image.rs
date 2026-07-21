use axum::{
    extract::{Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use sqlx::MySqlPool;

use crate::repository::room_image_repository;

pub async fn serve(State(pool): State<MySqlPool>, Path(uuid): Path<String>) -> impl IntoResponse {
    match room_image_repository::find_by_uuid(&pool, &uuid).await {
        Ok(Some((data, mime_type))) => ([(header::CONTENT_TYPE, mime_type)], data).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to load room image");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn stamp_card(
    State(pool): State<MySqlPool>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    use crate::repository::{player_repository, room_repository};
    use crate::services::game_service::{self, GameServiceError};

    let result: Result<Option<(Vec<String>, i64)>, GameServiceError> =
        game_service::with_db_call_timeout(async {
            let Some(player) = player_repository::find_by_stamp_card_token(&pool, &token).await?
            else {
                return Ok(None);
            };
            let room_names =
                room_repository::find_visited_room_names_ordered(&pool, player.id).await?;
            let total_rooms = room_repository::count(&pool, player.event_id).await?;
            Ok(Some((room_names, total_rooms)))
        })
        .await;

    let (room_names, total_rooms) = match result {
        Ok(Some(data)) => data,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!(?err, "failed to load stamp card data");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let png = crate::services::stamp_card_service::render_png(&room_names, total_rooms);

    (
        [
            (header::CONTENT_TYPE, "image/png".to_string()),
            (header::CACHE_CONTROL, "private, max-age=60".to_string()),
        ],
        png,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
        routing::get,
    };
    use image::{ImageBuffer, Rgba};
    use tower::ServiceExt;

    fn app_state(pool: sqlx::MySqlPool) -> crate::AppState {
        crate::AppState::new(
            pool,
            "test-channel-secret",
            "test-channel-access-token",
            "https://example.test",
            "test-liff-id",
            "test-login-channel-id",
            None,
        )
    }

    fn png_bytes(color: Rgba<u8>) -> Vec<u8> {
        let image = ImageBuffer::from_pixel(96, 96, color);
        let mut output = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut output, image::ImageFormat::Png)
            .unwrap();
        output.into_inner()
    }

    #[sqlx::test]
    async fn public_image_returns_stored_image(pool: sqlx::MySqlPool) {
        let data = b"jpeg-bytes";
        crate::repository::room_image_repository::insert(
            &pool,
            "public-image-uuid",
            data,
            "image/jpeg",
        )
        .await
        .unwrap();
        let app = Router::new()
            .route("/public/image/{uuid}", get(super::serve))
            .with_state(pool);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/public/image/public-image-uuid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/jpeg"
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], data);
    }

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
    async fn public_stamp_card_returns_png_for_valid_token(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room_id = crate::repository::room_repository::insert(
            &pool,
            event_id,
            "Library",
            "Quest",
            None,
            None,
            None,
            None,
            None,
            "qr-stamp-handler",
        )
        .await
        .unwrap();
        let player_id = crate::repository::player_repository::insert(
            &pool,
            "line-stamp-handler",
            event_id,
            "Alice",
            "stamp-token-handler",
        )
        .await
        .unwrap();
        crate::repository::player_repository::insert_visited_room(&pool, player_id, room_id)
            .await
            .unwrap();
        let app = Router::new()
            .route("/public/stamp-card/{token}", get(super::stamp_card))
            .with_state(app_state(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/public/stamp-card/stamp-token-handler")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/png"
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(image::guess_format(&body).unwrap(), image::ImageFormat::Png);
    }

    #[sqlx::test]
    async fn public_stamp_card_returns_png_for_custom_stamp_image(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let stamp_image_id = crate::repository::room_image_repository::insert(
            &pool,
            "handler-stamp-image",
            &png_bytes(Rgba([0x21, 0x9E, 0xBC, 255])),
            "image/png",
        )
        .await
        .unwrap();
        let room_id = crate::repository::room_repository::insert(
            &pool,
            event_id,
            "Library",
            "Quest",
            None,
            None,
            None,
            Some("図書"),
            Some(stamp_image_id),
            "qr-stamp-handler-custom",
        )
        .await
        .unwrap();
        let player_id = crate::repository::player_repository::insert(
            &pool,
            "line-stamp-handler-custom",
            event_id,
            "Alice",
            "stamp-token-handler-custom",
        )
        .await
        .unwrap();
        crate::repository::player_repository::insert_visited_room(&pool, player_id, room_id)
            .await
            .unwrap();
        let app = Router::new()
            .route("/public/stamp-card/{token}", get(super::stamp_card))
            .with_state(app_state(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/public/stamp-card/stamp-token-handler-custom")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/png"
        );
    }

    #[sqlx::test]
    async fn public_stamp_card_returns_not_found_for_missing_token(pool: sqlx::MySqlPool) {
        let app = Router::new()
            .route("/public/stamp-card/{token}", get(super::stamp_card))
            .with_state(app_state(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/public/stamp-card/missing-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test]
    async fn public_image_returns_not_found_for_missing_uuid(pool: sqlx::MySqlPool) {
        let app = Router::new()
            .route("/public/image/{uuid}", get(super::serve))
            .with_state(pool);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/public/image/missing-uuid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
