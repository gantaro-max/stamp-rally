#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
        routing::{get, post},
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;

    const PUBLIC_BASE_URL: &str = "https://example.test";

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

    async fn seed_room(pool: &sqlx::MySqlPool, event_id: i32, name: &str, qr_uuid: &str) -> i32 {
        crate::repository::room_repository::insert(
            pool,
            event_id,
            name,
            &format!("Quest for {name}"),
            Some("red"),
            Some("hint"),
            None,
            qr_uuid,
        )
        .await
        .unwrap()
    }

    async fn seed_player(pool: &sqlx::MySqlPool, event_id: i32, line_user_id: &str, room_id: i32) -> i32 {
        let player_id = crate::repository::player_repository::insert(pool, line_user_id, event_id, "Alice")
            .await
            .unwrap();
        crate::repository::player_repository::update_current_room(pool, player_id, room_id)
            .await
            .unwrap();
        player_id
    }

    fn test_app(pool: sqlx::MySqlPool) -> Router {
        let mut state = crate::AppState::new(
            pool,
            "test-channel-secret",
            "test-channel-access-token",
            PUBLIC_BASE_URL,
            "test-liff-id",
            "test-login-channel-id",
        );
        state.verify_id_tokens = false;
        state.send_line_replies = false;
        Router::new()
            .route("/liff/checkin", get(super::checkin_page).post(super::checkin))
            .with_state(state)
    }

    async fn post_checkin(app: Router, id_token: &str, qr_uuid: &str) -> (StatusCode, Value) {
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/liff/checkin")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"id_token": id_token, "qr_uuid": qr_uuid}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[sqlx::test]
    async fn post_checkin_returns_next_and_updates_current_room(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let current_room = seed_room(&pool, event_id, "Library", "qr-handler-current").await;
        let next_room = seed_room(&pool, event_id, "Gym", "qr-handler-next").await;
        seed_player(&pool, event_id, "line-handler-next", current_room).await;
        let app = test_app(pool.clone());

        let (status, body) = post_checkin(app, "line-handler-next", "qr-handler-current").await;
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-handler-next",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({"status":"next"}));
        assert_eq!(player.current_room_id, Some(next_room));
    }

    #[sqlx::test]
    async fn post_checkin_returns_cleared_and_sets_finished_at(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let only_room = seed_room(&pool, event_id, "Library", "qr-handler-clear").await;
        seed_player(&pool, event_id, "line-handler-clear", only_room).await;
        let app = test_app(pool.clone());

        let (status, body) = post_checkin(app, "line-handler-clear", "qr-handler-clear").await;
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-handler-clear",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, json!({"status":"cleared"}));
        assert!(player.finished_at.is_some());
    }

    #[sqlx::test]
    async fn post_checkin_returns_room_not_found(pool: sqlx::MySqlPool) {
        seed_event(&pool).await;
        let app = test_app(pool);

        let (status, body) = post_checkin(app, "line-missing-room", "missing-qr").await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body, json!({"status":"rejected","reason":"room_not_found"}));
    }

    #[sqlx::test]
    async fn post_checkin_returns_wrong_room(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let current_room = seed_room(&pool, event_id, "Library", "qr-handler-current-wrong").await;
        seed_room(&pool, event_id, "Gym", "qr-handler-wrong").await;
        seed_player(&pool, event_id, "line-handler-wrong", current_room).await;
        let app = test_app(pool);

        let (status, body) = post_checkin(app, "line-handler-wrong", "qr-handler-wrong").await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body, json!({"status":"rejected","reason":"wrong_room"}));
    }

    #[sqlx::test]
    async fn get_checkin_page_contains_liff_id(pool: sqlx::MySqlPool) {
        let app = test_app(pool);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/liff/checkin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("test-liff-id"));
    }
}
