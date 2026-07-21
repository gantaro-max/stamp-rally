use askama::Template;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    AppState,
    services::{game_service, line_client},
};

#[derive(Template)]
#[template(path = "liff/checkin.html")]
struct CheckinTemplate {
    liff_id: String,
}

#[derive(Debug, Deserialize)]
pub struct CheckinRequest {
    id_token: String,
    qr_uuid: String,
}

pub async fn checkin_page(State(state): State<AppState>) -> Response {
    let template = CheckinTemplate {
        liff_id: state.liff_id.to_string(),
    };

    match template.render() {
        Ok(body) => Html(body).into_response(),
        Err(err) => {
            tracing::error!(%err, "failed to render LIFF checkin page");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn checkin(State(state): State<AppState>, Json(body): Json<CheckinRequest>) -> Response {
    let line_user_id = if state.verify_id_tokens {
        match line_client::verify_id_token(
            &state.http_client,
            &body.id_token,
            &state.line_login_channel_id,
        )
        .await
        {
            Ok(claims) => claims.sub,
            Err(err) => {
                tracing::warn!(?err, "failed to verify LIFF id token");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"status": "rejected", "reason": "invalid_id_token"})),
                )
                    .into_response();
            }
        }
    } else {
        body.id_token
    };

    let outcome = match game_service::with_db_call_timeout(game_service::checkin(
        &state.pool,
        &state.public_base_url,
        &line_user_id,
        &body.qr_uuid,
    ))
    .await
    {
        Ok(outcome) => outcome,
        Err(err) => {
            tracing::error!(?err, "failed to process LIFF checkin");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    match outcome {
        game_service::CheckinOutcome::NextQuest(reply) => {
            if state.send_line_replies {
                let messages = line_client::to_line_messages(&reply, &state.liff_id);
                if let Err(err) = line_client::push_message(
                    &state.http_client,
                    &state.line_channel_access_token,
                    &line_user_id,
                    messages,
                )
                .await
                {
                    tracing::error!(?err, "failed to push next quest message");
                }
            }
            (StatusCode::OK, Json(json!({"status": "next"}))).into_response()
        }
        game_service::CheckinOutcome::Cleared(reply) => {
            if state.send_line_replies {
                let messages = line_client::to_line_messages(&reply, &state.liff_id);
                if let Err(err) = line_client::push_message(
                    &state.http_client,
                    &state.line_channel_access_token,
                    &line_user_id,
                    messages,
                )
                .await
                {
                    tracing::error!(?err, "failed to push cleared message");
                }
            }
            (StatusCode::OK, Json(json!({"status": "cleared"}))).into_response()
        }
        game_service::CheckinOutcome::Rejected(reason) => {
            let (status, reason) = rejection_response(reason);
            (
                status,
                Json(json!({"status": "rejected", "reason": reason})),
            )
                .into_response()
        }
    }
}

fn rejection_response(reason: game_service::CheckinRejection) -> (StatusCode, &'static str) {
    match reason {
        game_service::CheckinRejection::RoomNotFound => (StatusCode::NOT_FOUND, "room_not_found"),
        game_service::CheckinRejection::NotRegistered => (StatusCode::FORBIDDEN, "not_registered"),
        game_service::CheckinRejection::AlreadyFinished => {
            (StatusCode::FORBIDDEN, "already_finished")
        }
        game_service::CheckinRejection::WrongRoom => (StatusCode::FORBIDDEN, "wrong_room"),
        game_service::CheckinRejection::AnswerNotVerified => {
            (StatusCode::FORBIDDEN, "answer_not_verified")
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
    use serde_json::{Value, json};
    use tower::ServiceExt;

    const PUBLIC_BASE_URL: &str = "https://example.test";
    const REJECTION_MESSAGES: &[(&str, &str)] = &[
        (
            "wrong_room",
            "このQRコードはご案内している部屋のものではありません。LINEチャットで案内されている部屋をご確認ください。",
        ),
        ("already_finished", "既に全部屋クリア済みです。"),
        (
            "not_registered",
            "参加登録が完了していません。LINEで「開始」と送信してください。",
        ),
        (
            "answer_not_verified",
            "先にLINEで正解を送信してから、QRコードを読み込んでください。",
        ),
        (
            "room_not_found",
            "無効なQRコードです。もう一度お試しください。",
        ),
        (
            "invalid_id_token",
            "認証に失敗しました。時間をおいてもう一度お試しください。",
        ),
    ];

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
            None,            None,
            None,

            qr_uuid,)
        .await
        .unwrap()
    }

    async fn seed_player(
        pool: &sqlx::MySqlPool,
        event_id: i32,
        line_user_id: &str,
        room_id: i32,
    ) -> i32 {
        let player_id = crate::repository::player_repository::insert(
            pool,
            line_user_id,
            event_id,
            "Alice",
            &format!("stamp-token-{line_user_id}"),
        )
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
            None,
        );
        state.verify_id_tokens = false;
        state.send_line_replies = false;
        Router::new()
            .route(
                "/liff/checkin",
                get(super::checkin_page).post(super::checkin),
            )
            .with_state(state)
    }

    fn test_pool() -> sqlx::MySqlPool {
        sqlx::mysql::MySqlPoolOptions::new()
            .connect_lazy("mysql://user:password@localhost/database")
            .unwrap()
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

    async fn get_checkin_page_body(app: Router) -> (StatusCode, String) {
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/liff/checkin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap())
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

    #[tokio::test]
    async fn get_checkin_page_contains_liff_id() {
        let app = test_app(test_pool());

        let (status, body) = get_checkin_page_body(app).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("test-liff-id"));
        assert!(body.contains(
            "integrity=\"sha384-QWTKZyjpPEjISv5WaRU9OFeRpok6YctnYmDr5pNlyT2bRjXh0JMhjY6hW+ALEwIH\""
        ));
        assert!(body.contains("crossorigin=\"anonymous\""));
    }

    #[tokio::test]
    async fn get_checkin_page_contains_reason_specific_rejection_messages() {
        let app = test_app(test_pool());

        let (status, body) = get_checkin_page_body(app).await;

        assert_eq!(status, StatusCode::OK);
        for (reason, message) in REJECTION_MESSAGES {
            assert!(body.contains(reason), "missing reason: {reason}");
            assert!(
                body.contains(message),
                "missing rejection message: {message}"
            );
        }
        assert!(body.contains("reasonMessages[body.reason]"));
        assert!(
            body.contains("チェックインできませんでした。LINEチャットの案内を確認してください。")
        );
    }

    #[tokio::test]
    async fn get_checkin_page_keeps_success_messages() {
        let app = test_app(test_pool());

        let (status, body) = get_checkin_page_body(app).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("body.status === 'next'"));
        assert!(body.contains("チェックインしました。次の案内はLINEチャットを確認してください。"));
        assert!(body.contains("body.status === 'cleared'"));
        assert!(body.contains("クリアしました。LINEチャットを確認してください。"));
    }

    #[tokio::test]
    async fn get_checkin_page_contains_close_window_button() {
        let app = test_app(test_pool());

        let (status, body) = get_checkin_page_body(app).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("id=\"close-button\""));
        assert!(body.contains("LINEチャットに戻る"));
        assert!(body.contains("d-none"));
        assert!(body.contains("liff.closeWindow()"));
        assert!(body.contains("closeButton.classList.remove('d-none')"));
    }
}
