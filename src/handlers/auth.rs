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

use crate::services::{auth_service, csrf_service};

#[derive(Template)]
#[template(path = "auth/login.html")]
struct LoginTemplate {
    csrf_token: String,
    error_message: Option<&'static str>,
}

#[derive(Deserialize)]
pub struct LoginForm {
    password: String,
    csrf_token: Option<String>,
}

#[derive(Deserialize)]
pub struct LogoutForm {
    csrf_token: Option<String>,
}

pub async fn login_form(session: Session) -> Response {
    render_login(session, None).await
}

pub async fn login(
    State(pool): State<MySqlPool>,
    session: Session,
    Form(form): Form<LoginForm>,
) -> Response {
    if !csrf_service::verify_token(&session, form.csrf_token.as_deref().unwrap_or("")).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    match auth_service::try_login(&pool, &form.password).await {
        Ok(true) => {
            if session.insert("admin_authenticated", true).await.is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            if session.cycle_id().await.is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            redirect_to("/admin/dashboard")
        }
        Ok(false) => render_login(session, Some("パスワードが正しくありません")).await,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub async fn logout(session: Session, Form(form): Form<LogoutForm>) -> Response {
    if !csrf_service::verify_token(&session, form.csrf_token.as_deref().unwrap_or("")).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    if session.flush().await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    redirect_to("/auth/login")
}

async fn render_login(session: Session, error_message: Option<&'static str>) -> Response {
    let csrf_token = match csrf_service::issue_token(&session).await {
        Ok(token) => token,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let template = LoginTemplate {
        csrf_token,
        error_message,
    };

    match template.render() {
        Ok(body) => Html(body).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn redirect_to(location: &'static str) -> Response {
    (StatusCode::FOUND, [(header::LOCATION, location)]).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::{Body, to_bytes}, http::Request};
    use sqlx::MySqlPool;
    use tower::ServiceExt;

    async fn login_client(pool: MySqlPool) -> (Router, String, String) {
        crate::services::auth_service::seed_admin_event_if_empty(&pool, "admin-secret", "Stamp Rally")
            .await
            .unwrap();
        let app = crate::app_router(pool);
        let response = app.clone().oneshot(Request::builder().uri("/auth/login").body(Body::empty()).unwrap()).await.unwrap();
        let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap().split(';').next().unwrap().to_owned();
        let body = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
        let csrf = body.split("name=\"csrf_token\" value=\"").nth(1).unwrap().split('"').next().unwrap().to_owned();
        (app, cookie, csrf)
    }

    #[sqlx::test]
    async fn case29_concurrent_attempts_are_rate_limited(pool: MySqlPool) {
        let (app, cookie, csrf) = login_client(pool).await;
        let request = || Request::builder()
            .method("POST")
            .uri("/auth/login")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::COOKIE, &cookie)
            .header("x-forwarded-for", "203.0.113.1")
            .body(Body::from(format!("password=wrong&csrf_token={csrf}")))
            .unwrap();
        let (a, b, c, d, e, f) = tokio::join!(
            app.clone().oneshot(request()), app.clone().oneshot(request()),
            app.clone().oneshot(request()), app.clone().oneshot(request()),
            app.clone().oneshot(request()), app.clone().oneshot(request()),
        );
        assert!([a.unwrap(), b.unwrap(), c.unwrap(), d.unwrap(), e.unwrap(), f.unwrap()]
            .iter().any(|response| response.status() == StatusCode::TOO_MANY_REQUESTS));
    }
}
