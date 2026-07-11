use askama::Template;
use axum::{
    Form,
    extract::State,
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use sqlx::MySqlPool;
use tower_sessions::Session;

use crate::services::{csrf_service, event_service, ranking_service, room_service};

#[derive(Template)]
#[template(path = "admin/dashboard.html")]
struct DashboardTemplate {
    csrf_token: String,
    is_team_mode: bool,
    require_answer_check: bool,
    room_count: usize,
}

pub async fn dashboard(session: Session, State(pool): State<MySqlPool>) -> Response {
    let csrf_token = match csrf_service::issue_token(&session).await {
        Ok(token) => token,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let event = match event_service::current(&pool).await {
        Ok(event) => event,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let rooms = match room_service::list(&pool, event.id).await {
        Ok(rooms) => rooms,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let template = DashboardTemplate {
        csrf_token,
        is_team_mode: event.is_team_mode,
        require_answer_check: event.require_answer_check,
        room_count: rooms.len(),
    };

    match template.render() {
        Ok(body) => Html(body).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Template)]
#[template(path = "admin/settings.html")]
struct SettingsTemplate {
    csrf_token: String,
    is_team_mode: bool,
    require_answer_check: bool,
}

#[derive(Debug, Deserialize)]
pub struct SettingsForm {
    #[serde(default, deserialize_with = "deserialize_checkbox")]
    is_team_mode: bool,
    #[serde(default, deserialize_with = "deserialize_checkbox")]
    require_answer_check: bool,
    #[serde(default)]
    csrf_token: String,
}

fn deserialize_checkbox<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    Ok(value == "on" || value == "true")
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
    Form(form): Form<SettingsForm>,
) -> Response {
    if !csrf_service::verify_token(&session, &form.csrf_token).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let input = event_service::SettingsInput {
        is_team_mode: form.is_team_mode,
        require_answer_check: form.require_answer_check,
    };
    match event_service::update_settings(&pool, input).await {
        Ok(()) => (StatusCode::FOUND, [(header::LOCATION, "/admin/settings")]).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
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
