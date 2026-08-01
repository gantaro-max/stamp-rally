use askama::Template;
use axum::{
    extract::{Multipart, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use sqlx::MySqlPool;
use tower_sessions::Session;

use crate::AppState;
use crate::handlers::image::public_image_url;
use crate::repository::room_image_repository;
use crate::services::{csrf_service, event_service, qr_service, ranking_service, room_service};

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
struct DashboardTemplate {
    csrf_token: String,
    is_team_mode: bool,
    require_answer_check: bool,
    room_count: usize,
    line_add_friend_url: Option<String>,
}

pub async fn dashboard(session: Session, State(state): State<AppState>) -> Response {
    let csrf_token = match csrf_service::issue_token(&session).await {
        Ok(token) => token,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let event = match event_service::current(&state.pool).await {
        Ok(event) => event,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let rooms = match room_service::list(&state.pool, event.id).await {
        Ok(rooms) => rooms,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let template = DashboardTemplate {
        csrf_token,
        is_team_mode: event.is_team_mode,
        require_answer_check: event.require_answer_check,
        room_count: rooms.len(),
        line_add_friend_url: state.line_add_friend_url.as_deref().map(str::to_owned),
    };

    match template.render() {
        Ok(body) => Html(body).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn line_qr(State(state): State<AppState>) -> Response {
    let Some(url) = state.line_add_friend_url.as_deref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let png = qr_service::render_png(url);

    ([(header::CONTENT_TYPE, "image/png")], png).into_response()
}

#[derive(Template)]
#[template(path = "admin/settings.html")]
struct SettingsTemplate {
    csrf_token: String,
    is_team_mode: bool,
    require_answer_check: bool,
    stamp_card_background_image_url: Option<String>,
}

#[derive(Debug, Default)]
pub struct SettingsForm {
    is_team_mode: bool,
    require_answer_check: bool,
    csrf_token: String,
    stamp_card_background_image_bytes: Option<Vec<u8>>,
}

pub async fn settings_form(session: Session, State(pool): State<MySqlPool>) -> Response {
    let csrf_token = match csrf_service::issue_token(&session).await {
        Ok(token) => token,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let event = match event_service::current(&pool).await {
        Ok(event) => event,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let stamp_card_background_image_url =
        match public_image_url_for_id(&pool, event.stamp_card_background_image_id).await {
            Ok(url) => url,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
    let template = SettingsTemplate {
        csrf_token,
        is_team_mode: event.is_team_mode,
        require_answer_check: event.require_answer_check,
        stamp_card_background_image_url,
    };

    match template.render() {
        Ok(body) => Html(body).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn public_image_url_for_id(
    pool: &MySqlPool,
    image_id: Option<i32>,
) -> Result<Option<String>, sqlx::Error> {
    let Some(image_id) = image_id else {
        return Ok(None);
    };
    Ok(room_image_repository::find_uuid_by_id(pool, image_id)
        .await?
        .map(|uuid| public_image_url(&uuid)))
}

pub async fn update_settings(
    session: Session,
    State(pool): State<MySqlPool>,
    multipart: Multipart,
) -> Response {
    let form = match parse_settings_multipart(multipart).await {
        Ok(form) => form,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    if !csrf_service::verify_token(&session, &form.csrf_token).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let input = event_service::SettingsInput {
        is_team_mode: form.is_team_mode,
        require_answer_check: form.require_answer_check,
        stamp_card_background_image_bytes: form.stamp_card_background_image_bytes,
    };
    match event_service::update_settings(&pool, input).await {
        Ok(()) => (StatusCode::FOUND, [(header::LOCATION, "/admin/settings")]).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn parse_settings_multipart(mut multipart: Multipart) -> Result<SettingsForm, ()> {
    let mut form = SettingsForm::default();

    while let Some(field) = multipart.next_field().await.map_err(|_| ())? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "stamp_card_background_image" {
            let bytes = field.bytes().await.map_err(|_| ())?;
            if !bytes.is_empty() {
                form.stamp_card_background_image_bytes = Some(bytes.to_vec());
            }
            continue;
        }

        let value = field.text().await.map_err(|_| ())?;
        match name.as_str() {
            "is_team_mode" => form.is_team_mode = true,
            "require_answer_check" => form.require_answer_check = true,
            "csrf_token" => form.csrf_token = value,
            _ => {}
        }
    }

    Ok(form)
}

#[derive(Template)]
#[template(path = "admin/ranking.html")]
struct RankingTemplate {
    csrf_token: String,
    ranking: ranking_service::RankingView,
}

pub async fn ranking(session: Session, State(pool): State<MySqlPool>) -> Response {
    let csrf_token = match csrf_service::issue_token(&session).await {
        Ok(token) => token,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let event = match event_service::current(&pool).await {
        Ok(event) => event,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let ranking = match ranking_service::get_ranking(&pool, event.id).await {
        Ok(ranking) => ranking,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let template = RankingTemplate {
        csrf_token,
        ranking,
    };

    match template.render() {
        Ok(body) => Html(body).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use askama::Template;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
        middleware as axum_middleware,
        routing::get,
    };
    use time::Duration;
    use tower::ServiceExt;
    use tower_sessions::{Expiry, MemoryStore, Session, SessionManagerLayer};

    use super::SettingsTemplate;

    async fn seed_authenticated_session(session: Session) -> &'static str {
        session.insert("admin_authenticated", true).await.unwrap();
        "ok"
    }

    async fn authenticated_cookie(app: Router) -> String {
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test/session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .next_back()
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    #[test]
    fn settings_template_previews_configured_stamp_card_background() {
        let template = SettingsTemplate {
            csrf_token: "logout-csrf".to_string(),
            is_team_mode: false,
            require_answer_check: false,
            stamp_card_background_image_url: Some("/public/image/background-uuid".to_string()),
        };

        let body = template.render().unwrap();

        assert!(body.contains(r#"<img src="/public/image/background-uuid"#));
    }

    #[test]
    fn settings_template_shows_unset_when_stamp_card_background_is_missing() {
        let template = SettingsTemplate {
            csrf_token: "logout-csrf".to_string(),
            is_team_mode: false,
            require_answer_check: false,
            stamp_card_background_image_url: None,
        };

        let body = template.render().unwrap();

        assert!(body.contains("未設定"));
    }

    #[sqlx::test]
    async fn settings_form_previews_existing_stamp_card_background(pool: sqlx::MySqlPool) {
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
        let image_id = crate::repository::room_image_repository::insert(
            &pool,
            "background-preview-uuid",
            b"background-image",
            "image/png",
        )
        .await
        .unwrap();
        crate::repository::event_repository::update_settings(
            &pool,
            event_id,
            false,
            false,
            Some(image_id),
        )
        .await
        .unwrap();

        let session_layer = SessionManagerLayer::new(MemoryStore::default())
            .with_expiry(Expiry::OnInactivity(Duration::hours(12)));
        let app = Router::new()
            .route("/test/session", get(seed_authenticated_session))
            .route(
                "/admin/settings",
                get(super::settings_form).route_layer(axum_middleware::from_fn(
                    crate::middleware::require_admin::require_admin,
                )),
            )
            .with_state(pool)
            .layer(session_layer);
        let cookie = authenticated_cookie(app.clone()).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/admin/settings")
                    .header(header::COOKIE, cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains(r#"/public/image/background-preview-uuid"#));
    }
}
