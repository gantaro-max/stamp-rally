use askama::Template;
use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode, header},
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

fn client_ip(headers: &HeaderMap) -> &str {
    headers.get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit(',').next())
        .map(str::trim)
        .unwrap_or("unknown")
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
    use axum::http::HeaderMap;

    fn forwarded_for(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", value.parse().unwrap());
        headers
    }

    #[test]
    fn case12_single_forwarded_ip() {
        assert_eq!(client_ip(&forwarded_for("203.0.113.1")), "203.0.113.1");
    }
    #[test]
    fn case13_uses_rightmost_forwarded_ip() {
        assert_eq!(client_ip(&forwarded_for("198.51.100.9, 203.0.113.1")), "203.0.113.1");
    }

    #[test]
    fn case14_trims_forwarded_ip_whitespace() {
        assert_eq!(client_ip(&forwarded_for("198.51.100.9 , 203.0.113.1 ")), "203.0.113.1");
    }

    #[test]
    fn case15_missing_forwarded_header_uses_unknown() {
        assert_eq!(client_ip(&HeaderMap::new()), "unknown");
    }

    #[test]
    fn case16_empty_forwarded_values_use_unknown() {
        for value in ["", ",", " , , ", "   "] {
            assert_eq!(client_ip(&forwarded_for(value)), "unknown");
        }
        assert_eq!(client_ip(&forwarded_for("198.51.100.9, 203.0.113.1, , ")), "203.0.113.1");
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", axum::http::HeaderValue::from_bytes(&[0xff]).unwrap());
        assert_eq!(client_ip(&headers), "unknown");
    }

}
