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
        .and_then(|value| value.rsplit(',').map(str::trim).find(|ip| !ip.is_empty()))
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

    struct LoginClient {
        app: axum::Router,
        cookie: String,
        csrf_token: String,
    }

    impl LoginClient {
        async fn new(pool: MySqlPool) -> Self {
            auth_service::seed_admin_event_if_empty(&pool, "admin-secret", "Stamp Rally")
                .await.unwrap();
            let state = crate::AppState::new(pool, "secret", "token", "https://example.test", "liff", "channel", None);
            Self::with_state(state).await
        }

        async fn with_state(state: crate::AppState) -> Self {
            use tower::ServiceExt;
            let app = axum::Router::new()
                .route("/auth/login", axum::routing::get(login_form).post(login))
                .with_state(state)
                .layer(tower_sessions::SessionManagerLayer::new(tower_sessions::MemoryStore::default()));
            let response = app.clone().oneshot(axum::http::Request::builder()
                .uri("/auth/login").body(axum::body::Body::empty()).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap()
                .split(';').next().unwrap().to_owned();
            let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let body = String::from_utf8(body.to_vec()).unwrap();
            let csrf_token = body.split("name=\"csrf_token\" value=\"").nth(1).unwrap()
                .split('"').next().unwrap().to_owned();
            Self { app, cookie, csrf_token }
        }

        async fn post(&mut self, ip: &str, password: &str, valid_csrf: bool) -> Response {
            use tower::ServiceExt;
            let csrf = if valid_csrf { &self.csrf_token } else { "invalid-token" };
            let response = self.app.clone().oneshot(axum::http::Request::builder()
                .method("POST").uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &self.cookie)
                .header("x-forwarded-for", ip)
                .body(axum::body::Body::from(format!("password={password}&csrf_token={csrf}")))
                .unwrap()).await.unwrap();
            if let Some(cookie) = response.headers().get(header::SET_COOKIE) {
                self.cookie = cookie.to_str().unwrap().split(';').next().unwrap().to_owned();
            }
            response
        }

        async fn fail(&mut self, ip: &str, count: usize) {
            for _ in 0..count {
                let response = self.post(ip, "wrong-password", true).await;
                assert_eq!(response.status(), StatusCode::OK);
                let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
                assert!(String::from_utf8(body.to_vec()).unwrap().contains("パスワードが正しくありません"));
            }
        }
    }

    #[sqlx::test]
    async fn case17_correct_fifth_attempt_succeeds(pool: MySqlPool) {
        let mut client = LoginClient::new(pool).await;
        client.fail("203.0.113.1", 4).await;
        let old_cookie = client.cookie.clone();
        let response = client.post("203.0.113.1", "admin-secret", true).await;
        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(response.headers()[header::LOCATION], "/admin/dashboard");
        assert_ne!(client.cookie, old_cookie);
    }

    #[sqlx::test]
    async fn case18_five_failures_reject_even_correct_password(pool: MySqlPool) {
        let mut client = LoginClient::new(pool).await;
        client.fail("203.0.113.1", 5).await;
        let response = client.post("203.0.113.1", "admin-secret", true).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("試行回数の上限に達しました"));
        assert!(body.contains("name=\"csrf_token\""));
    }

}
