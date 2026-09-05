use askama::Template;
use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use tower_sessions::Session;

use crate::{
    AppState,
    services::{auth_service, csrf_service, login_attempt_service},
};

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

fn client_ip(headers: &HeaderMap) -> &str {
    headers
        .get_all("x-forwarded-for")
        .iter()
        .next_back()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit(',').map(str::trim).find(|ip| !ip.is_empty()))
        .unwrap_or("unknown")
}

fn retry_after_seconds(remaining: chrono::Duration) -> i64 {
    let seconds = remaining.num_seconds();
    seconds + i64::from(remaining > chrono::Duration::seconds(seconds))
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
    let now = chrono::Utc::now();
    let attempt = {
        let mut records = match state.login_attempts.lock() {
            Ok(records) => records,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        login_attempt_service::cleanup(&mut records, now);
        login_attempt_service::register_attempt(&mut records, key, now)
    };
    if let login_attempt_service::AttemptResult::Blocked(remaining) = attempt {
        let mut response = render_login(
            session,
            Some("試行回数の上限に達しました。しばらく待ってから再度お試しください"),
        )
        .await;
        *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
        response.headers_mut().insert(
            header::RETRY_AFTER,
            retry_after_seconds(remaining).to_string().parse().unwrap(),
        );
        return response;
    }

    match auth_service::try_login(&state.pool, &form.password).await {
        Ok(true) => {
            if let Ok(mut records) = state.login_attempts.lock() {
                login_attempt_service::record_success(&mut records, key);
            } else {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
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
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::Request,
    };
    use sqlx::MySqlPool;
    use tower::ServiceExt;

    async fn login_client(pool: MySqlPool) -> (Router, String, String) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app = crate::app_router(pool);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let body = String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        let csrf = body
            .split("name=\"csrf_token\" value=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_owned();
        (app, cookie, csrf)
    }

    struct TestLoginClient {
        app: Router,
        cookie: String,
        csrf: String,
    }

    impl TestLoginClient {
        async fn new(pool: MySqlPool) -> Self {
            crate::services::auth_service::seed_admin_event_if_empty(
                &pool,
                "admin-secret",
                "Stamp Rally",
            )
            .await
            .unwrap();
            Self::from_state(crate::AppState::new(
                pool,
                "secret",
                "token",
                "https://example.test",
                "liff",
                "channel",
                None,
            ))
            .await
        }

        async fn from_state(state: crate::AppState) -> Self {
            let app = crate::app_router_with_state(state.clone());
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/auth/login")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let cookie = response.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned();
            let body = String::from_utf8(
                to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .to_vec(),
            )
            .unwrap();
            let csrf = body
                .split("name=\"csrf_token\" value=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_owned();
            Self { app, cookie, csrf }
        }

        async fn post(&mut self, ip: &str, password: &str, valid_csrf: bool) -> Response {
            let csrf = if valid_csrf { &self.csrf } else { "invalid" };
            let response = self
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/auth/login")
                        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                        .header(header::COOKIE, &self.cookie)
                        .header("x-forwarded-for", ip)
                        .body(Body::from(format!("password={password}&csrf_token={csrf}")))
                        .unwrap(),
                )
                .await
                .unwrap();
            if let Some(cookie) = response.headers().get(header::SET_COOKIE) {
                self.cookie = cookie
                    .to_str()
                    .unwrap()
                    .split(';')
                    .next()
                    .unwrap()
                    .to_owned();
            }
            response
        }

        async fn fail(&mut self, ip: &str, count: usize) {
            for _ in 0..count {
                assert_eq!(self.post(ip, "wrong", true).await.status(), StatusCode::OK);
            }
        }
    }

    #[sqlx::test]
    async fn case29_concurrent_attempts_are_rate_limited(pool: MySqlPool) {
        let (app, cookie, csrf) = login_client(pool).await;
        let request = || {
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &cookie)
                .header("x-forwarded-for", "203.0.113.1")
                .body(Body::from(format!("password=wrong&csrf_token={csrf}")))
                .unwrap()
        };
        let (a, b, c, d, e, f) = tokio::join!(
            app.clone().oneshot(request()),
            app.clone().oneshot(request()),
            app.clone().oneshot(request()),
            app.clone().oneshot(request()),
            app.clone().oneshot(request()),
            app.clone().oneshot(request()),
        );
        assert!(
            [
                a.unwrap(),
                b.unwrap(),
                c.unwrap(),
                d.unwrap(),
                e.unwrap(),
                f.unwrap()
            ]
            .iter()
            .any(|response| response.status() == StatusCode::TOO_MANY_REQUESTS)
        );
    }

    #[sqlx::test]
    async fn case21_correct_fifth_attempt_succeeds(pool: MySqlPool) {
        let mut client = TestLoginClient::new(pool).await;
        client.fail("203.0.113.1", 4).await;
        assert_eq!(
            client
                .post("203.0.113.1", "admin-secret", true)
                .await
                .status(),
            StatusCode::FOUND
        );
    }

    #[sqlx::test]
    async fn case22_correct_password_is_blocked_after_five_attempts(pool: MySqlPool) {
        let mut client = TestLoginClient::new(pool).await;
        client.fail("203.0.113.1", 5).await;
        assert_eq!(
            client
                .post("203.0.113.1", "admin-secret", true)
                .await
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[sqlx::test]
    async fn case23_blocked_response_has_retry_after(pool: MySqlPool) {
        let mut client = TestLoginClient::new(pool).await;
        client.fail("203.0.113.1", 5).await;
        let response = client.post("203.0.113.1", "admin-secret", true).await;
        let seconds: u64 = response.headers()[header::RETRY_AFTER]
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!((1..=900).contains(&seconds));
    }

    #[test]
    fn case24_retry_after_rounds_up_fractional_seconds() {
        assert_eq!(retry_after_seconds(chrono::Duration::milliseconds(1)), 1);
        assert_eq!(
            retry_after_seconds(chrono::Duration::milliseconds(899_001)),
            900
        );
    }

    #[sqlx::test]
    async fn case25_other_sender_can_log_in(pool: MySqlPool) {
        let mut client = TestLoginClient::new(pool).await;
        client.fail("203.0.113.1", 5).await;
        assert_eq!(
            client
                .post("203.0.113.2", "admin-secret", true)
                .await
                .status(),
            StatusCode::FOUND
        );
    }

    #[sqlx::test]
    async fn case26_invalid_csrf_is_not_counted(pool: MySqlPool) {
        let mut client = TestLoginClient::new(pool).await;
        for _ in 0..5 {
            assert_eq!(
                client.post("203.0.113.1", "wrong", false).await.status(),
                StatusCode::FORBIDDEN
            );
        }
        assert_eq!(
            client
                .post("203.0.113.1", "admin-secret", true)
                .await
                .status(),
            StatusCode::FOUND
        );
    }

    #[sqlx::test]
    async fn case27_success_clears_attempts(pool: MySqlPool) {
        let mut client = TestLoginClient::new(pool).await;
        client.fail("203.0.113.1", 4).await;
        assert_eq!(
            client
                .post("203.0.113.1", "admin-secret", true)
                .await
                .status(),
            StatusCode::FOUND
        );
        client.fail("203.0.113.1", 4).await;
        assert_eq!(
            client
                .post("203.0.113.1", "admin-secret", true)
                .await
                .status(),
            StatusCode::FOUND
        );
    }

    fn closed_pool_state() -> crate::AppState {
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .connect_lazy("mysql://user:password@localhost/database")
            .unwrap();
        crate::AppState::new(
            pool,
            "secret",
            "token",
            "https://example.test",
            "liff",
            "channel",
            None,
        )
    }

    #[tokio::test]
    async fn case28_blocked_attempt_does_not_touch_database() {
        let state = closed_pool_state();
        state.pool.close().await;
        for _ in 0..5 {
            login_attempt_service::register_attempt(
                &mut state.login_attempts.lock().unwrap(),
                "203.0.113.1",
                chrono::Utc::now(),
            );
        }
        let mut client = TestLoginClient::from_state(state).await;
        assert_eq!(
            client.post("203.0.113.1", "anything", true).await.status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[sqlx::test]
    async fn case30_concurrent_attempt_count_never_exceeds_five(pool: MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let state = crate::AppState::new(
            pool,
            "secret",
            "token",
            "https://example.test",
            "liff",
            "channel",
            None,
        );
        let client = TestLoginClient::from_state(state.clone()).await;
        let request = || {
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, &client.cookie)
                .header("x-forwarded-for", "203.0.113.1")
                .body(Body::from(format!(
                    "password=wrong&csrf_token={}",
                    client.csrf
                )))
                .unwrap()
        };
        let _ = tokio::join!(
            client.app.clone().oneshot(request()),
            client.app.clone().oneshot(request()),
            client.app.clone().oneshot(request()),
            client.app.clone().oneshot(request()),
            client.app.clone().oneshot(request()),
            client.app.clone().oneshot(request())
        );
        assert_eq!(
            state.login_attempts.lock().unwrap()["203.0.113.1"].attempts,
            5
        );
    }

    #[tokio::test]
    async fn case31_database_error_remains_counted() {
        let state = closed_pool_state();
        state.pool.close().await;
        let mut client = TestLoginClient::from_state(state.clone()).await;
        assert_eq!(
            client.post("203.0.113.1", "anything", true).await.status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            state.login_attempts.lock().unwrap()["203.0.113.1"].attempts,
            1
        );
    }

    #[tokio::test]
    async fn case32_five_database_errors_block_the_next_attempt() {
        let state = closed_pool_state();
        state.pool.close().await;
        let mut client = TestLoginClient::from_state(state).await;
        for _ in 0..5 {
            assert_eq!(
                client.post("203.0.113.1", "anything", true).await.status(),
                StatusCode::INTERNAL_SERVER_ERROR
            );
        }
        assert_eq!(
            client
                .post("203.0.113.1", "admin-secret", true)
                .await
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    fn forwarded_for(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", value.parse().unwrap());
        headers
    }

    #[test]
    fn cases13_to_17_extract_or_default_the_forwarded_ip() {
        assert_eq!(client_ip(&forwarded_for("203.0.113.1")), "203.0.113.1");
        assert_eq!(
            client_ip(&forwarded_for("198.51.100.9, 203.0.113.1")),
            "203.0.113.1"
        );
        assert_eq!(
            client_ip(&forwarded_for("198.51.100.9 , 203.0.113.1 ")),
            "203.0.113.1"
        );
        assert_eq!(client_ip(&HeaderMap::new()), "unknown");
        for value in ["", ",", " , "] {
            assert_eq!(client_ip(&forwarded_for(value)), "unknown");
        }
    }

    #[test]
    fn cases18_to_20_only_trust_the_last_forwarded_header_line() {
        let mut headers = forwarded_for("198.51.100.9");
        headers.append("x-forwarded-for", "192.0.2.9, 203.0.113.1".parse().unwrap());
        assert_eq!(client_ip(&headers), "203.0.113.1");
        headers.append(
            "x-forwarded-for",
            axum::http::HeaderValue::from_bytes(&[0xff]).unwrap(),
        );
        assert_eq!(client_ip(&headers), "unknown");
        let mut headers = forwarded_for("198.51.100.9");
        headers.append("x-forwarded-for", " , ".parse().unwrap());
        assert_eq!(client_ip(&headers), "unknown");
    }
}
