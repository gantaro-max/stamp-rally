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
        crate::repository::room_image_repository::insert(&pool, "public-image-uuid", data, "image/jpeg")
            .await
            .unwrap();
        let app = Router::new().route("/public/image/{uuid}", get(super::serve)).with_state(pool);

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
        assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/jpeg");
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], data);
    }

    #[sqlx::test]
    async fn public_image_returns_not_found_for_missing_uuid(pool: sqlx::MySqlPool) {
        let app = Router::new().route("/public/image/{uuid}", get(super::serve)).with_state(pool);

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
