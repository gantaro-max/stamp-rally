use askama::Template;
use axum::{
    extract::{Multipart, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use sqlx::MySqlPool;
use tower_sessions::Session;

use crate::AppState;
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
    let template = SettingsTemplate {
        csrf_token,
        is_team_mode: event.is_team_mode,
        require_answer_check: event.require_answer_check,
    };

    match template.render() {
        Ok(body) => Html(body).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
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
