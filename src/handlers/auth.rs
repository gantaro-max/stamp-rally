use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use tower_sessions::Session;

use crate::services::csrf_service;

#[derive(Template)]
#[template(path = "auth/login.html")]
struct LoginTemplate {
    csrf_token: String,
    error_message: Option<&'static str>,
}

pub async fn login_form(session: Session) -> Response {
    render_login(session, None).await
}

async fn render_login(session: Session, error_message: Option<&'static str>) -> Response {
    let csrf_token = csrf_service::issue_token(&session).await;
    let template = LoginTemplate {
        csrf_token,
        error_message,
    };

    match template.render() {
        Ok(body) => Html(body).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
