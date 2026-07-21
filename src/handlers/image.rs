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

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
        routing::get,
    };
    use tower::ServiceExt;

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
            .with_state(pool);

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
    async fn public_stamp_card_returns_not_found_for_missing_token(pool: sqlx::MySqlPool) {
        let app = Router::new()
            .route("/public/stamp-card/{token}", get(super::stamp_card))
            .with_state(pool);

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
