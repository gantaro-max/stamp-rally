use askama::Template;
use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use tower_sessions::Session;

use crate::{AppState, services::{auth_service, csrf_service, login_attempt_service}};

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

fn retry_after_seconds(remaining: chrono::Duration) -> i64 {
    let seconds = remaining.num_seconds();
    seconds + i64::from(remaining > chrono::Duration::seconds(seconds))
}

fn client_ip(headers: &HeaderMap) -> &str {
    headers.get_all("x-forwarded-for")
        .iter().rev()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.rsplit(','))
        .map(str::trim)
        .find(|ip| !ip.is_empty())
        .unwrap_or("unknown")
}

pub async fn login_form(session: Session) -> Response {
    render_login(session, None).await
}

pub async fn login(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Response {
    if !csrf_service::verify_token(&session, form.csrf_token.as_deref().unwrap_or("")).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let key = client_ip(&headers);
    let blocked = {
        let mut records = match state.login_attempts.lock() {
            Ok(records) => records,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        let now = chrono::Utc::now();
        login_attempt_service::cleanup(&mut records, now);
        login_attempt_service::blocked_for(&records, key, now)
    };
    if let Some(remaining) = blocked {
        let mut response = render_login(session, Some("試行回数の上限に達しました。しばらく待ってから再度お試しください")).await;
        *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
        response.headers_mut().insert(header::RETRY_AFTER,
            retry_after_seconds(remaining).to_string().parse().expect("integer is a valid header value"));
        return response;
    }

    match auth_service::try_login(&state.pool, &form.password).await {
        Ok(true) => {
            {
                let mut records = match state.login_attempts.lock() {
                    Ok(records) => records,
                    Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                };
                login_attempt_service::record_success(&mut records, key);
            }
            if session.insert("admin_authenticated", true).await.is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            if session.cycle_id().await.is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            redirect_to("/admin/dashboard")
        }
        Ok(false) => {
            {
                let mut records = match state.login_attempts.lock() {
                    Ok(records) => records,
                    Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                };
                login_attempt_service::record_failure(&mut records, key, chrono::Utc::now());
            }
            render_login(session, Some("パスワードが正しくありません")).await
        }
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
    use sqlx::MySqlPool;

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

    #[sqlx::test]
    async fn case19_blocked_response_has_retry_after(pool: MySqlPool) {
        let mut client = LoginClient::new(pool).await;
        client.fail("203.0.113.1", 5).await;
        let response = client.post("203.0.113.1", "admin-secret", true).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let seconds: u64 = response.headers().get(header::RETRY_AFTER)
            .expect("blocked responses must include Retry-After")
            .to_str().unwrap().parse().unwrap();
        assert!((1..=900).contains(&seconds));
    }

    #[sqlx::test]
    async fn case20_another_sender_can_login_while_one_is_blocked(pool: MySqlPool) {
        let mut client = LoginClient::new(pool).await;
        client.fail("198.51.100.9, 203.0.113.1", 5).await;
        assert_eq!(client.post("192.0.2.99, 203.0.113.1", "admin-secret", true).await.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(client.post("203.0.113.2", "admin-secret", true).await.status(), StatusCode::FOUND);
        assert_eq!(client.post("203.0.113.1", "admin-secret", true).await.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[sqlx::test]
    async fn case21_invalid_csrf_does_not_count_as_login_failure(pool: MySqlPool) {
        let mut client = LoginClient::new(pool).await;
        for _ in 0..5 {
            assert_eq!(client.post("203.0.113.1", "wrong-password", false).await.status(), StatusCode::FORBIDDEN);
        }
        assert_eq!(client.post("203.0.113.1", "admin-secret", true).await.status(), StatusCode::FOUND);
    }

    #[sqlx::test]
    async fn case22_success_resets_subsequent_failure_count(pool: MySqlPool) {
        let mut client = LoginClient::new(pool).await;
        client.fail("203.0.113.1", 4).await;
        assert_eq!(client.post("203.0.113.1", "admin-secret", true).await.status(), StatusCode::FOUND);
        client.fail("203.0.113.1", 4).await;
        assert_eq!(client.post("203.0.113.1", "admin-secret", true).await.status(), StatusCode::FOUND);
        client.fail("203.0.113.1", 5).await;
        assert_eq!(client.post("203.0.113.1", "admin-secret", true).await.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn blocked_login_cleans_expired_records_without_touching_database() {
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .connect_lazy("mysql://user:password@localhost/database").unwrap();
        pool.close().await;
        let state = crate::AppState::new(pool, "secret", "token", "https://example.test", "liff", "channel", None);
        let now = chrono::Utc::now();
        {
            let mut records = state.login_attempts.lock().unwrap();
            login_attempt_service::record_failure(&mut records, "expired", now - chrono::Duration::minutes(15));
            for _ in 0..5 {
                login_attempt_service::record_failure(&mut records, "203.0.113.1", now);
            }
        }
        let mut client = LoginClient::with_state(state.clone()).await;
        assert_eq!(client.post("203.0.113.1", "anything", false).await.status(), StatusCode::FORBIDDEN);
        assert!(state.login_attempts.lock().unwrap().contains_key("expired"));
        assert_eq!(client.post("203.0.113.1", "anything", true).await.status(), StatusCode::TOO_MANY_REQUESTS);
        let records = state.login_attempts.lock().unwrap();
        assert!(!records.contains_key("expired"));
        assert_eq!(records["203.0.113.1"].failures, 5);
        assert_eq!(records["203.0.113.1"].last_failure, now);
    }

    #[test]
    fn repeated_forwarded_headers_use_the_last_nonempty_ip() {
        let mut headers = forwarded_for("198.51.100.9");
        headers.append("x-forwarded-for", "192.0.2.9, 203.0.113.1".parse().unwrap());
        headers.append("x-forwarded-for", " , ".parse().unwrap());
        assert_eq!(client_ip(&headers), "203.0.113.1");
    }

}
