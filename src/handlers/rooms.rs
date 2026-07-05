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
