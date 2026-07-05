use askama::Template;
use axum::{
    extract::{Multipart, Path as AxumPath, State},
    http::{StatusCode, header},
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


struct RoomMultipartForm {
    room_name: String,
    quest_text: String,
    answer: Option<String>,
    hint_msg: Option<String>,
    image_bytes: Option<Vec<u8>>,
    csrf_token: Option<String>,
}

impl RoomMultipartForm {
    fn values(&self) -> AddTemplateValues {
        AddTemplateValues {
            room_name: self.room_name.clone(),
            quest_text: self.quest_text.clone(),
            answer: self.answer.clone().unwrap_or_default(),
            hint_msg: self.hint_msg.clone().unwrap_or_default(),
        }
    }
}

pub async fn add(
    State(pool): State<MySqlPool>,
    session: Session,
    multipart: Multipart,
) -> Response {
    let Some(event) = (match crate::repository::event_repository::find_singleton(&pool).await {
        Ok(event) => event,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let form = match parse_room_multipart(multipart).await {
        Ok(form) => form,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };


    if !csrf_service::verify_token(&session, form.csrf_token.as_deref().unwrap_or("")).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let values = form.values();
    let input = room_service::CreateRoomInput {
        room_name: form.room_name,
        quest_text: form.quest_text,
        answer: form.answer,
        hint_msg: form.hint_msg,
        image_bytes: form.image_bytes,
    };

    match room_service::create(&pool, event.id, input).await {
        Ok(_) => redirect_to("/admin/rooms"),
        Err(room_service::RoomError::MaxRoomsReached) => {
            render_add_form(session, event.require_answer_check, Some("部屋数の上限に達しています"), values).await
        }
        Err(room_service::RoomError::AnswerRequired) => {
            render_add_form(session, event.require_answer_check, Some("正解を入力してください"), values).await
        }
        Err(room_service::RoomError::Image(_)) => {
            render_add_form(session, event.require_answer_check, Some("画像を確認してください"), values).await
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn parse_room_multipart(mut multipart: Multipart) -> Result<RoomMultipartForm, ()> {
    let mut form = RoomMultipartForm {
        room_name: String::new(),
        quest_text: String::new(),
        answer: None,
        hint_msg: None,
        image_bytes: None,
        csrf_token: None,
    };

    while let Some(field) = multipart.next_field().await.map_err(|_| ())? {
        let name = field.name().unwrap_or_default().to_string();
        if name == "image" {
            let bytes = field.bytes().await.map_err(|_| ())?;
            if !bytes.is_empty() {
                form.image_bytes = Some(bytes.to_vec());
            }
            continue;
        }
        let value = field.text().await.map_err(|_| ())?;
        match name.as_str() {
            "room_name" => form.room_name = value,
            "quest_text" => form.quest_text = value,
            "answer" => form.answer = non_empty(value),
            "hint_msg" => form.hint_msg = non_empty(value),
            "csrf_token" => form.csrf_token = Some(value),
            _ => {}
        }
    }

    Ok(form)
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn redirect_to(location: &'static str) -> Response {
    (StatusCode::FOUND, [(header::LOCATION, location)]).into_response()
}


#[derive(Template)]
#[template(path = "admin/rooms/edit.html")]
struct RoomEditTemplate {
    room_id: i32,
    csrf_token: String,
    require_answer_check: bool,
    room_name: String,
    quest_text: String,
    answer: String,
    hint_msg: String,
    error_message: Option<&'static str>,
}

pub async fn edit_form(
    State(pool): State<MySqlPool>,
    session: Session,
    AxumPath(id): AxumPath<i32>,
) -> Response {
    let Some(event) = (match crate::repository::event_repository::find_singleton(&pool).await {
        Ok(event) => event,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Some(room) = (match room_service::get(&pool, id).await {
        Ok(room) => room,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    render_edit_form(
        session,
        event.require_answer_check,
        None,
        RoomEditTemplateValues {
            room_id: room.id,
            room_name: room.room_name,
            quest_text: room.quest_text,
            answer: room.answer.unwrap_or_default(),
            hint_msg: room.hint_msg.unwrap_or_default(),
        },
    )
    .await
}

struct RoomEditTemplateValues {
    room_id: i32,
    room_name: String,
    quest_text: String,
    answer: String,
    hint_msg: String,
}

async fn render_edit_form(
    session: Session,
    require_answer_check: bool,
    error_message: Option<&'static str>,
    values: RoomEditTemplateValues,
) -> Response {
    let csrf_token = match csrf_service::issue_token(&session).await {
        Ok(token) => token,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let template = RoomEditTemplate {
        room_id: values.room_id,
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
