use askama::Template;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use sqlx::MySqlPool;
use tower_sessions::Session;

use crate::{
    repository::room_repository::Room,
    services::{csrf_service, room_service},
};

#[derive(Template)]
#[template(path = "admin/rooms/list.html")]
struct RoomListTemplate {
    rooms: Vec<Room>,
    csrf_token: String,
}

pub async fn list(State(pool): State<MySqlPool>, session: Session) -> Response {
    let csrf_token = match csrf_service::issue_token(&session).await {
        Ok(token) => token,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some(event) = (match crate::repository::event_repository::find_singleton(&pool).await {
        Ok(event) => event,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let rooms = match room_service::list(&pool, event.id).await {
        Ok(rooms) => rooms,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let template = RoomListTemplate { rooms, csrf_token };

    match template.render() {
        Ok(body) => Html(body).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Template)]
#[template(path = "admin/rooms/add.html")]
struct RoomAddTemplate {
    csrf_token: String,
    require_answer_check: bool,
    room_name: String,
    quest_text: String,
    answer: String,
    hint_msg: String,
    error_message: Option<&'static str>,
}

pub async fn add_form(State(pool): State<MySqlPool>, session: Session) -> Response {
    let Some(event) = (match crate::repository::event_repository::find_singleton(&pool).await {
        Ok(event) => event,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    render_add_form(session, event.require_answer_check, None, default_add_template_values()).await
}

struct AddTemplateValues {
    room_name: String,
    quest_text: String,
    answer: String,
    hint_msg: String,
}

fn default_add_template_values() -> AddTemplateValues {
    AddTemplateValues {
        room_name: String::new(),
        quest_text: String::new(),
        answer: String::new(),
        hint_msg: String::new(),
    }
}

async fn render_add_form(
    session: Session,
    require_answer_check: bool,
    error_message: Option<&'static str>,
    values: AddTemplateValues,
) -> Response {
    let csrf_token = match csrf_service::issue_token(&session).await {
        Ok(token) => token,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let template = RoomAddTemplate {
        csrf_token,
        require_answer_check,
        room_name: values.room_name,
        quest_text: values.quest_text,
        answer: values.answer,
        hint_msg: values.hint_msg,
        error_message,
    };

    match template.render() {
        Ok(body) => Html(body).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
