mod handlers;
mod middleware;
mod repository;
mod services;

use axum::{
    Router, middleware as axum_middleware,
    routing::{get, post},
};
use sqlx::{MySqlPool, mysql::MySqlPoolOptions};
use std::{env, net::SocketAddr, process};
use time::Duration;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let database_url = env::var("DATABASE_URL").unwrap_or_else(|err| {
        eprintln!("DATABASE_URL must be set: {err}");
        process::exit(1);
    });

    let pool = MySqlPoolOptions::new()
        .connect(&database_url)
        .await
        .unwrap_or_else(|err| {
            eprintln!("failed to connect to database: {err}");
            process::exit(1);
        });

    seed_admin_event_or_exit(&pool).await;

    let app = app_router(pool);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|err| {
            eprintln!("failed to bind {addr}: {err}");
            process::exit(1);
        });

    tracing::info!(%addr, "listening");
    axum::serve(listener, app).await.unwrap_or_else(|err| {
        eprintln!("server error: {err}");
        process::exit(1);
    });
}

async fn seed_admin_event_or_exit(pool: &MySqlPool) {
    let event_count = repository::event_repository::count(pool)
        .await
        .unwrap_or_else(|err| {
            eprintln!("failed to count events: {err}");
            process::exit(1);
        });

    if event_count > 0 {
        return;
    }

    let admin_password = env::var("ADMIN_PASSWORD")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            eprintln!("ADMIN_PASSWORD must be set when events table is empty");
            process::exit(1);
        });

    services::auth_service::seed_admin_event_if_empty(pool, &admin_password, "Stamp Rally")
        .await
        .unwrap_or_else(|err| {
            eprintln!("failed to seed initial event: {err}");
            process::exit(1);
        });
}

fn app_router(pool: MySqlPool) -> Router {
    let session_layer = SessionManagerLayer::new(MemoryStore::default())
        .with_expiry(Expiry::OnInactivity(Duration::hours(12)));

    let admin_router = Router::new()
        .route("/dashboard", get(handlers::admin::dashboard))
        .route("/rooms", get(handlers::rooms::list))
        .route("/rooms/add", get(handlers::rooms::add_form))
        .route_layer(axum_middleware::from_fn(
            middleware::require_admin::require_admin,
        ));

    let logout_router = Router::new().route(
        "/logout",
        post(handlers::auth::logout).route_layer(axum_middleware::from_fn(
            middleware::require_admin::require_admin,
        )),
    );

    Router::new()
        .route("/health", get(handlers::health::health))
        .route(
            "/auth/login",
            get(handlers::auth::login_form).post(handlers::auth::login),
        )
        .nest("/auth", logout_router)
        .nest("/admin", admin_router)
        .with_state(pool)
        .layer(session_layer)
}

#[cfg(test)]
mod tests {
    use super::app_router;
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode, header},
        middleware as axum_middleware,
        routing::{get, post},
    };
    use sqlx::mysql::MySqlPoolOptions;
    use time::Duration;
    use tower::ServiceExt;
    use tower_sessions::{Expiry, MemoryStore, Session, SessionManagerLayer};

    fn extract_cookie(response: &axum::response::Response) -> String {
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .next_back()
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    fn extract_csrf_token(body: &str) -> String {
        let marker = r#"name="csrf_token" value=""#;
        let start = body.find(marker).unwrap() + marker.len();
        let rest = &body[start..];
        let end = rest.find('\"').unwrap();
        rest[..end].to_string()
    }

    async fn get_login_cookie_and_csrf(app: Router) -> (String, String) {
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/auth/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = extract_cookie(&response);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        (cookie, extract_csrf_token(&body))
    }

    async fn seed_authenticated_logout_session(session: Session) -> &'static str {
        session.insert("admin_authenticated", true).await.unwrap();
        session.insert("csrf_token", "valid-token").await.unwrap();
        "ok"
    }

    fn logout_test_app() -> Router {
        let session_layer = SessionManagerLayer::new(MemoryStore::default())
            .with_expiry(Expiry::OnInactivity(Duration::hours(12)));

        Router::new()
            .route("/test/session", get(seed_authenticated_logout_session))
            .route(
                "/auth/logout",
                post(crate::handlers::auth::logout).route_layer(axum_middleware::from_fn(
                    crate::middleware::require_admin::require_admin,
                )),
            )
            .layer(session_layer)
    }

    fn test_pool() -> sqlx::MySqlPool {
        MySqlPoolOptions::new()
            .connect_lazy("mysql://user:password@localhost/database")
            .unwrap()
    }

    #[tokio::test]
    async fn app_router_registers_health_route() {
        let response = app_router(test_pool())
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"ok");
    }

    #[tokio::test]
    async fn login_form_returns_post_form_with_csrf_token() {
        let response = app_router(test_pool())
            .oneshot(
                Request::builder()
                    .uri("/auth/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains(r#"action="/auth/login""#));
        assert!(body.contains(r#"name="csrf_token""#));
    }

    #[sqlx::test]
    async fn login_with_correct_password_redirects_and_sets_session(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app = app_router(pool);
        let (cookie, csrf_token) = get_login_cookie_and_csrf(app.clone()).await;

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, cookie)
                    .body(Body::from(format!(
                        "password=admin-secret&csrf_token={csrf_token}"
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/admin/dashboard"
        );
        assert!(response.headers().get(header::SET_COOKIE).is_some());
    }

    #[sqlx::test]
    async fn login_with_wrong_password_rerenders_error(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app = app_router(pool);
        let (cookie, csrf_token) = get_login_cookie_and_csrf(app.clone()).await;

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, cookie)
                    .body(Body::from(format!(
                        "password=wrong&csrf_token={csrf_token}"
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("パスワードが正しくありません"));
    }

    #[tokio::test]
    async fn login_rejects_missing_or_mismatched_csrf_token() {
        for body in [
            "password=admin-secret",
            "password=admin-secret&csrf_token=",
            "password=admin-secret&csrf_token=wrong",
        ] {
            let response = app_router(test_pool())
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/auth/login")
                        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
    }

    #[tokio::test]
    async fn admin_dashboard_redirects_when_not_logged_in() {
        let response = app_router(test_pool())
            .oneshot(
                Request::builder()
                    .uri("/admin/dashboard")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/auth/login"
        );
    }

    #[tokio::test]
    async fn admin_rooms_redirects_when_not_logged_in() {
        let response = app_router(test_pool())
            .oneshot(
                Request::builder()
                    .uri("/admin/rooms")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/auth/login"
        );
    }

    #[sqlx::test]
    async fn authenticated_session_can_access_dashboard(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app = app_router(pool);
        let (cookie, csrf_token) = get_login_cookie_and_csrf(app.clone()).await;
        let login_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, cookie)
                    .body(Body::from(format!(
                        "password=admin-secret&csrf_token={csrf_token}"
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        let session_cookie = extract_cookie(&login_response);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/admin/dashboard")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"ok");
    }

    #[sqlx::test]
    async fn authenticated_session_can_view_rooms(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let event_id = crate::repository::event_repository::find_singleton(&pool)
            .await
            .unwrap()
            .unwrap()
            .id;
        crate::repository::room_repository::insert(
            &pool,
            event_id,
            "Library",
            "Find a book",
            None,
            None,
            None,
            "qr-list-1",
        )
        .await
        .unwrap();
        let app = app_router(pool);
        let (cookie, csrf_token) = get_login_cookie_and_csrf(app.clone()).await;
        let login_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, cookie)
                    .body(Body::from(format!(
                        "password=admin-secret&csrf_token={csrf_token}"
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        let session_cookie = extract_cookie(&login_response);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/admin/rooms")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("Library"));
    }

    #[sqlx::test]
    async fn authenticated_session_can_view_room_add_form(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app = app_router(pool);
        let (cookie, csrf_token) = get_login_cookie_and_csrf(app.clone()).await;
        let login_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, cookie)
                    .body(Body::from(format!(
                        "password=admin-secret&csrf_token={csrf_token}"
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        let session_cookie = extract_cookie(&login_response);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/admin/rooms/add")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains(r#"action=\"/admin/rooms/add\""#));
        assert!(body.contains(r#"name=\"csrf_token\""#));
    }

    #[tokio::test]
    async fn logout_redirects_when_not_logged_in() {
        let response = app_router(test_pool())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/logout")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("csrf_token=anything"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/auth/login"
        );
    }

    #[sqlx::test]
    async fn logout_flushes_authenticated_session(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app = app_router(pool);
        let (cookie, csrf_token) = get_login_cookie_and_csrf(app.clone()).await;
        let login_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, cookie)
                    .body(Body::from(format!(
                        "password=admin-secret&csrf_token={csrf_token}"
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        let session_cookie = extract_cookie(&login_response);

        let logout_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/auth/logout")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, session_cookie.clone())
                    .body(Body::from(format!("csrf_token={csrf_token}")))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(logout_response.status(), StatusCode::FOUND);
        assert_eq!(
            logout_response.headers().get(header::LOCATION).unwrap(),
            "/auth/login"
        );

        let dashboard_response = app
            .oneshot(
                Request::builder()
                    .uri("/admin/dashboard")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(dashboard_response.status(), StatusCode::FOUND);
        assert_eq!(
            dashboard_response.headers().get(header::LOCATION).unwrap(),
            "/auth/login"
        );
    }

    #[tokio::test]
    async fn logout_rejects_missing_or_mismatched_csrf_token() {
        let app = logout_test_app();
        let setup_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/test/session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let session_cookie = extract_cookie(&setup_response);

        for body in ["csrf_token=wrong", "csrf_token=", ""] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/auth/logout")
                        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                        .header(header::COOKIE, session_cookie.clone())
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
    }
}
