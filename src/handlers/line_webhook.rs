#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode, header},
        routing::post,
    };
    use base64::{Engine as _, engine::general_purpose};
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    use tower::ServiceExt;

    fn line_signature(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    #[sqlx::test]
    async fn callback_with_valid_text_message_updates_game_state(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let mut state = crate::AppState::new(
            pool,
            "test-channel-secret",
            "test-channel-access-token",
            "https://example.test",
        );
        state.send_line_replies = false;
        let pending = state.pending_registrations.clone();
        let app = Router::new().route("/callback", post(super::callback)).with_state(state);
        let body = r#"{"events":[{"type":"message","replyToken":"reply-token","source":{"userId":"line-valid"},"message":{"type":"text","text":"開始"}}]}"#;
        let body = body.as_bytes();
        let signature = line_signature("test-channel-secret", body);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/callback")
                    .header("x-line-signature", signature)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_vec()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(pending.lock().unwrap().contains("line-valid"));
    }
}
