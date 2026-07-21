use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;

use crate::{
    AppState,
    services::{game_service, line_client},
};

#[derive(Debug, Deserialize)]
struct WebhookPayload {
    events: Vec<WebhookEvent>,
}

#[derive(Debug, Deserialize)]
struct WebhookEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(rename = "replyToken")]
    reply_token: Option<String>,
    source: Option<WebhookSource>,
    message: Option<WebhookMessage>,
}

#[derive(Debug, Deserialize)]
struct WebhookSource {
    #[serde(rename = "userId")]
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WebhookMessage {
    #[serde(rename = "type")]
    message_type: String,
    text: Option<String>,
}

async fn dispatch<F>(spawn: bool, future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    if spawn {
        tokio::spawn(future);
    } else {
        future.await;
    }
}

async fn process_events(state: AppState, events: Vec<WebhookEvent>) {
    for event in events {
        if event.event_type != "message" {
            continue;
        }
        let Some(message) = event.message else {
            continue;
        };
        if message.message_type != "text" {
            continue;
        }
        let (Some(reply_token), Some(source), Some(text)) =
            (event.reply_token, event.source, message.text)
        else {
            continue;
        };
        let Some(user_id) = source.user_id else {
            continue;
        };

        let reply = match game_service::with_db_call_timeout(game_service::handle_text_message(
            &state.pool,
            &state.public_base_url,
            &user_id,
            &text,
        ))
        .await
        {
            Ok(reply) => reply,
            Err(err) => {
                tracing::error!(?err, "failed to handle LINE text message");
                continue;
            }
        };
        if !state.send_line_replies {
            continue;
        }
        let message = line_client::to_line_message(&reply, &state.liff_id);
        if let Err(err) = line_client::send_reply(
            &state.http_client,
            &state.line_channel_access_token,
            &reply_token,
            message,
        )
        .await
        {
            tracing::error!(?err, "failed to send LINE reply");
        }
    }
}

pub async fn callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Some(signature) = headers
        .get("x-line-signature")
        .and_then(|value| value.to_str().ok())
    else {
        return StatusCode::UNAUTHORIZED;
    };

    if !line_client::verify_signature(&state.line_channel_secret, &body, signature) {
        return StatusCode::UNAUTHORIZED;
    }

    let Ok(payload) = serde_json::from_slice::<WebhookPayload>(&body) else {
        return StatusCode::OK;
    };

    dispatch(
        state.spawn_background_tasks,
        process_events(state.clone(), payload.events),
    )
    .await;

    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use std::{
        future,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::Duration,
    };

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

    fn text_event(reply_token: &str, user_id: &str, text: &str) -> super::WebhookEvent {
        super::WebhookEvent {
            event_type: "message".to_string(),
            reply_token: Some(reply_token.to_string()),
            source: Some(super::WebhookSource {
                user_id: Some(user_id.to_string()),
            }),
            message: Some(super::WebhookMessage {
                message_type: "text".to_string(),
                text: Some(text.to_string()),
            }),
        }
    }

    #[tokio::test]
    async fn dispatch_with_spawn_returns_without_waiting_for_future() {
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            super::dispatch(true, future::pending()),
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn dispatch_without_spawn_waits_for_future() {
        let completed = Arc::new(AtomicBool::new(false));
        let completed_in_future = Arc::clone(&completed);

        super::dispatch(false, async move {
            completed_in_future.store(true, Ordering::SeqCst);
        })
        .await;

        assert!(completed.load(Ordering::SeqCst));
    }

    #[sqlx::test]
    async fn process_events_handles_same_user_messages_in_payload_order(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let event_id = crate::repository::event_repository::find_singleton(&pool)
            .await
            .unwrap()
            .unwrap()
            .id;
        let room_id = crate::repository::room_repository::insert(
            &pool,
            event_id,
            "Room A",
            "Quest A",
            None,
            None,
            None,
            "qr-room-a",
        )
        .await
        .unwrap();
        let mut state = crate::AppState::new(
            pool,
            "test-channel-secret",
            "test-channel-access-token",
            "https://example.test",
            "test-liff-id",
            "test-login-channel-id",
            None,
        );
        state.send_line_replies = false;
        let pool = state.pool.clone();

        super::process_events(
            state,
            vec![
                text_event("reply-start", "line-sequential", "開始"),
                text_event("reply-name", "line-sequential", "Alice"),
            ],
        )
        .await;

        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-sequential",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(player.player_name, "Alice");
        assert_eq!(player.current_room_id, Some(room_id));
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
            "test-liff-id",
            "test-login-channel-id",
            None,
        );
        state.send_line_replies = false;
        state.spawn_background_tasks = false;
        let event_id = crate::repository::event_repository::find_singleton(&state.pool)
            .await
            .unwrap()
            .unwrap()
            .id;
        let pool = state.pool.clone();
        let app = Router::new()
            .route("/callback", post(super::callback))
            .with_state(state);
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
        assert!(
            crate::repository::pending_registration_repository::exists(
                &pool,
                "line-valid",
                event_id
            )
            .await
            .unwrap()
        );
    }
}
