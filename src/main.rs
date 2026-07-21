mod handlers;
mod middleware;
mod repository;
mod services;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware as axum_middleware,
    routing::{get, post},
};
use sqlx::{MySqlPool, mysql::MySqlPoolOptions};
use std::{env, net::SocketAddr, process, sync::Arc};
use time::Duration;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

#[derive(Clone)]
pub struct AppState {
    pub pool: MySqlPool,
    pub line_channel_secret: Arc<str>,
    pub line_channel_access_token: Arc<str>,
    pub public_base_url: Arc<str>,
    pub liff_id: Arc<str>,
    pub line_login_channel_id: Arc<str>,
    pub line_add_friend_url: Option<Arc<str>>,
    pub verify_id_tokens: bool,
    pub http_client: reqwest::Client,
    pub send_line_replies: bool,
    pub spawn_background_tasks: bool,
}

impl AppState {
    fn new(
        pool: MySqlPool,
        line_channel_secret: impl Into<Arc<str>>,
        line_channel_access_token: impl Into<Arc<str>>,
        public_base_url: impl Into<Arc<str>>,
        liff_id: impl Into<Arc<str>>,
        line_login_channel_id: impl Into<Arc<str>>,
        line_add_friend_url: Option<String>,
    ) -> Self {
        Self {
            pool,
            line_channel_secret: line_channel_secret.into(),
            line_channel_access_token: line_channel_access_token.into(),
            public_base_url: public_base_url.into(),
            liff_id: liff_id.into(),
            line_login_channel_id: line_login_channel_id.into(),
            line_add_friend_url: line_add_friend_url.map(Into::into),
            verify_id_tokens: true,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("failed to build reqwest client"),
            send_line_replies: true,
            spawn_background_tasks: true,
        }
    }
}

impl axum::extract::FromRef<AppState> for MySqlPool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
    }
}

const ROOM_MULTIPART_BODY_LIMIT: usize = services::image_service::MAX_UPLOAD_BYTES + 64 * 1024;
const DEFAULT_PORT: u16 = 8000;

fn resolve_port(value: Option<&str>) -> u16 {
    value
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

fn resolve_line_add_friend_url(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let port = resolve_port(env::var("PORT").ok().as_deref());

    let database_url = env::var("DATABASE_URL").unwrap_or_else(|err| {
        eprintln!("DATABASE_URL must be set: {err}");
        process::exit(1);
    });

    let line_channel_secret = env::var("LINE_CHANNEL_SECRET").unwrap_or_else(|err| {
        eprintln!("LINE_CHANNEL_SECRET must be set: {err}");
        process::exit(1);
    });
    let line_channel_access_token = env::var("LINE_CHANNEL_ACCESS_TOKEN").unwrap_or_else(|err| {
        eprintln!("LINE_CHANNEL_ACCESS_TOKEN must be set: {err}");
        process::exit(1);
    });
    let public_base_url = env::var("PUBLIC_BASE_URL").unwrap_or_else(|err| {
        eprintln!("PUBLIC_BASE_URL must be set: {err}");
        process::exit(1);
    });
    let liff_id = env::var("LIFF_ID").unwrap_or_else(|err| {
        eprintln!("LIFF_ID must be set: {err}");
        process::exit(1);
    });
    let line_login_channel_id = env::var("LINE_LOGIN_CHANNEL_ID").unwrap_or_else(|err| {
        eprintln!("LINE_LOGIN_CHANNEL_ID must be set: {err}");
        process::exit(1);
    });
    let line_add_friend_url =
        resolve_line_add_friend_url(env::var("LINE_ADD_FRIEND_URL").ok().as_deref());

    let pool = MySqlPoolOptions::new()
        .connect(&database_url)
        .await
        .unwrap_or_else(|err| {
            eprintln!("failed to connect to database: {err}");
            process::exit(1);
        });

    seed_admin_event_or_exit(&pool).await;

    let app = app_router_with_state(AppState::new(
        pool,
        line_channel_secret,
        line_channel_access_token,
        public_base_url,
        liff_id,
        line_login_channel_id,
        line_add_friend_url,
    ));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
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

#[cfg(test)]
fn app_router(pool: MySqlPool) -> Router {
    app_router_with_state(AppState::new(
        pool,
        "test-channel-secret",
        "test-channel-access-token",
        "https://example.test",
        "test-liff-id",
        "test-login-channel-id",
        None,
    ))
}

fn app_router_with_state(state: AppState) -> Router {
    let session_layer = SessionManagerLayer::new(MemoryStore::default())
        .with_expiry(Expiry::OnInactivity(Duration::hours(12)));

    let admin_router = Router::new()
        .route("/dashboard", get(handlers::admin::dashboard))
        .route("/line-qr", get(handlers::admin::line_qr))
        .route(
            "/settings",
            get(handlers::admin::settings_form).post(handlers::admin::update_settings),
        )
        .route("/ranking", get(handlers::admin::ranking))
        .route("/rooms", get(handlers::rooms::list))
        .route(
            "/rooms/add",
            get(handlers::rooms::add_form).post(handlers::rooms::add),
        )
        .route("/rooms/edit/{id}", get(handlers::rooms::edit_form))
        .route("/rooms/update/{id}", post(handlers::rooms::update))
        .route("/rooms/delete/{id}", post(handlers::rooms::delete))
        .route("/rooms/{id}/qr", get(handlers::rooms::qr))
        .layer(DefaultBodyLimit::max(ROOM_MULTIPART_BODY_LIMIT))
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
        .route("/callback", post(handlers::line_webhook::callback))
        .route(
            "/liff/checkin",
            get(handlers::liff::checkin_page).post(handlers::liff::checkin),
        )
        .route("/public/image/{uuid}", get(handlers::image::serve))
        .route(
            "/public/stamp-card/{token}",
            get(handlers::image::stamp_card),
        )
        .route(
            "/auth/login",
            get(handlers::auth::login_form).post(handlers::auth::login),
        )
        .nest("/auth", logout_router)
        .nest("/admin", admin_router)
        .with_state(state)
        .layer(session_layer)
}

#[cfg(test)]
mod tests {
    use super::{AppState, app_router, app_router_with_state};
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

    async fn login_as_admin(app: Router) -> String {
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

        extract_cookie(&response)
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

    async fn consume_multipart_upload(mut multipart: axum::extract::Multipart) -> StatusCode {
        while let Some(field) = multipart.next_field().await.unwrap() {
            let _ = field.bytes().await.unwrap();
        }
        StatusCode::OK
    }

    fn room_upload_limit_test_app() -> Router {
        Router::new()
            .route("/test/upload", post(consume_multipart_upload))
            .layer(axum::extract::DefaultBodyLimit::max(
                super::ROOM_MULTIPART_BODY_LIMIT,
            ))
    }

    fn test_pool() -> sqlx::MySqlPool {
        MySqlPoolOptions::new()
            .connect_lazy("mysql://user:password@localhost/database")
            .unwrap()
    }

    fn app_router_with_line_add_friend_url(
        pool: sqlx::MySqlPool,
        line_add_friend_url: Option<String>,
    ) -> Router {
        app_router_with_state(AppState::new(
            pool,
            "test-channel-secret",
            "test-channel-access-token",
            "https://example.test",
            "test-liff-id",
            "test-login-channel-id",
            line_add_friend_url,
        ))
    }

    #[test]
    fn resolve_port_uses_numeric_env_value() {
        assert_eq!(super::resolve_port(Some("3000")), 3000);
    }

    #[test]
    fn resolve_port_defaults_to_8000_when_unset() {
        assert_eq!(super::resolve_port(None), 8000);
    }

    #[test]
    fn resolve_port_defaults_to_8000_when_invalid() {
        assert_eq!(super::resolve_port(Some("")), 8000);
        assert_eq!(super::resolve_port(Some("not-a-number")), 8000);
    }

    #[test]
    fn resolve_line_add_friend_url_ignores_unset_or_blank_values() {
        assert_eq!(super::resolve_line_add_friend_url(None), None);
        assert_eq!(super::resolve_line_add_friend_url(Some("")), None);
        assert_eq!(super::resolve_line_add_friend_url(Some("   ")), None);
    }

    #[test]
    fn resolve_line_add_friend_url_returns_trimmed_value() {
        assert_eq!(
            super::resolve_line_add_friend_url(Some(" https://lin.ee/test1234 ")),
            Some("https://lin.ee/test1234".to_string())
        );
    }

    #[test]
    fn sqlx_dependency_explicitly_enables_native_tls() {
        let manifest = std::fs::read_to_string("Cargo.toml").unwrap();

        assert!(manifest.contains(r#""tls-native-tls""#));
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

    #[tokio::test]
    async fn room_upload_routes_accept_multipart_larger_than_axum_default() {
        let app = room_upload_limit_test_app();
        let boundary = "large-room-upload";
        let mut body = Vec::new();
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"room.png\"\r\nContent-Type: image/png\r\n\r\n").as_bytes(),
        );
        body.extend(vec![b'a'; 2 * 1024 * 1024 + 1]);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/test/upload")
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_ne!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[sqlx::test]
    async fn authenticated_session_can_view_dashboard_with_empty_rooms(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app = app_router(pool);
        let session_cookie = login_as_admin(app.clone()).await;

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
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("個人戦"));
        assert!(body.contains("QR読み取りのみ"));
        assert!(body.contains("0 / 15部屋"));
        assert!(body.contains(r#"href="/admin/rooms""#));
        assert!(body.contains(r#"href="/admin/settings""#));
        assert!(body.contains(r#"href="/admin/ranking""#));
    }

    #[sqlx::test]
    async fn authenticated_dashboard_without_line_add_friend_url_shows_setup_message(
        pool: sqlx::MySqlPool,
    ) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app = app_router_with_line_add_friend_url(pool, None);
        let session_cookie = login_as_admin(app.clone()).await;

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
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("LINE_ADD_FRIEND_URL が未設定です"));
        assert!(!body.contains(r#"<img src="/admin/line-qr""#));
    }

    #[sqlx::test]
    async fn authenticated_dashboard_with_line_add_friend_url_shows_qr_image_and_url(
        pool: sqlx::MySqlPool,
    ) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app =
            app_router_with_line_add_friend_url(pool, Some("https://lin.ee/test1234".to_string()));
        let session_cookie = login_as_admin(app.clone()).await;

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
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains(r#"<img src="/admin/line-qr""#));
        assert!(body.contains("https://lin.ee/test1234"));
    }

    #[sqlx::test]
    async fn authenticated_line_qr_returns_png_for_line_add_friend_url(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app =
            app_router_with_line_add_friend_url(pool, Some("https://lin.ee/test1234".to_string()));
        let session_cookie = login_as_admin(app.clone()).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/admin/line-qr")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/png"
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(image::guess_format(&body).unwrap(), image::ImageFormat::Png);
        let image = image::load_from_memory(&body).unwrap().to_luma8();
        let mut prepared = rqrr::PreparedImage::prepare(image);
        let grids = prepared.detect_grids();
        let (_, decoded) = grids[0].decode().unwrap();

        assert_eq!(decoded, "https://lin.ee/test1234");
    }

    #[sqlx::test]
    async fn authenticated_line_qr_returns_not_found_without_line_add_friend_url(
        pool: sqlx::MySqlPool,
    ) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app = app_router_with_line_add_friend_url(pool, None);
        let session_cookie = login_as_admin(app.clone()).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/admin/line-qr")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn admin_line_qr_redirects_when_not_logged_in() {
        let response = app_router(test_pool())
            .oneshot(
                Request::builder()
                    .uri("/admin/line-qr")
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
    async fn authenticated_dashboard_shows_room_count(pool: sqlx::MySqlPool) {
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
            "qr-dashboard-1",
        )
        .await
        .unwrap();
        crate::repository::room_repository::insert(
            &pool,
            event_id,
            "Gallery",
            "Find a painting",
            None,
            None,
            None,
            "qr-dashboard-2",
        )
        .await
        .unwrap();
        let app = app_router(pool);
        let session_cookie = login_as_admin(app.clone()).await;

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
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("2 / 15部屋"));
    }

    #[sqlx::test]
    async fn authenticated_dashboard_shows_team_and_answer_check_modes(pool: sqlx::MySqlPool) {
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
        crate::repository::event_repository::update_settings(&pool, event_id, true, true)
            .await
            .unwrap();
        let app = app_router(pool);
        let session_cookie = login_as_admin(app.clone()).await;

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
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("チーム戦"));
        assert!(body.contains("QR＋正解入力"));
        assert!(!body.contains("個人戦"));
        assert!(!body.contains("QR読み取りのみ"));
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
        let room_id = crate::repository::room_repository::insert(
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
        assert!(body.contains(&format!(r#"src="/admin/rooms/{room_id}/qr""#)));
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
        assert!(body.contains(r#"action="/admin/rooms/add""#));
        assert!(body.contains(r#"name="csrf_token""#));
    }

    #[sqlx::test]
    async fn post_room_add_creates_room_and_redirects(pool: sqlx::MySqlPool) {
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
        let app = app_router(pool.clone());
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
        let boundary = "room-boundary";
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"csrf_token\"\r\n\r\n{csrf_token}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"room_name\"\r\n\r\nLibrary\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"quest_text\"\r\n\r\nFind a book\r\n--{boundary}--\r\n"
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/rooms/add")
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .header(header::COOKIE, session_cookie)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/admin/rooms"
        );
        assert_eq!(
            crate::repository::room_repository::count(&pool, event_id)
                .await
                .unwrap(),
            1
        );
    }

    #[sqlx::test]
    async fn post_room_add_rejects_invalid_csrf(pool: sqlx::MySqlPool) {
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
        let app = app_router(pool.clone());
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

        for csrf_part in [
            "",
            "--room-boundary\r\nContent-Disposition: form-data; name=\"csrf_token\"\r\n\r\nwrong\r\n",
        ] {
            let boundary = "room-boundary";
            let body = format!(
                "{csrf_part}--{boundary}\r\nContent-Disposition: form-data; name=\"room_name\"\r\n\r\nLibrary\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"quest_text\"\r\n\r\nFind a book\r\n--{boundary}--\r\n"
            );
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/admin/rooms/add")
                        .header(
                            header::CONTENT_TYPE,
                            format!("multipart/form-data; boundary={boundary}"),
                        )
                        .header(header::COOKIE, session_cookie.clone())
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        assert_eq!(
            crate::repository::room_repository::count(&pool, event_id)
                .await
                .unwrap(),
            0
        );
    }

    #[sqlx::test]
    async fn room_edit_returns_404_for_missing_room(pool: sqlx::MySqlPool) {
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
                    .uri("/admin/rooms/edit/9999")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test]
    async fn post_room_delete_removes_room_and_redirects(pool: sqlx::MySqlPool) {
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
        let room_id = crate::repository::room_repository::insert(
            &pool,
            event_id,
            "Delete Me",
            "Quest",
            None,
            None,
            None,
            "qr-delete-handler",
        )
        .await
        .unwrap();
        let app = app_router(pool.clone());
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
                    .method("POST")
                    .uri(format!("/admin/rooms/delete/{room_id}"))
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::from(format!("csrf_token={csrf_token}")))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/admin/rooms"
        );
        assert!(
            crate::repository::room_repository::find_by_id(&pool, room_id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test]
    async fn room_qr_returns_png(pool: sqlx::MySqlPool) {
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
        let room_id = crate::repository::room_repository::insert(
            &pool,
            event_id,
            "QR Room",
            "Quest",
            None,
            None,
            None,
            "qr-handler-value",
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
                    .uri(format!("/admin/rooms/{room_id}/qr"))
                    .header(header::COOKIE, session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/png"
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(image::guess_format(&body).unwrap(), image::ImageFormat::Png);
    }

    #[sqlx::test]
    async fn post_room_update_changes_room_and_redirects(pool: sqlx::MySqlPool) {
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
        let room_id = crate::repository::room_repository::insert(
            &pool,
            event_id,
            "Old",
            "Old Quest",
            None,
            None,
            None,
            "qr-update-handler",
        )
        .await
        .unwrap();
        let app = app_router(pool.clone());
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
        let boundary = "room-update-boundary";
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"csrf_token\"\r\n\r\n{csrf_token}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"room_name\"\r\n\r\nNew\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"quest_text\"\r\n\r\nNew Quest\r\n--{boundary}--\r\n"
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/admin/rooms/update/{room_id}"))
                    .header(
                        header::CONTENT_TYPE,
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .header(header::COOKIE, session_cookie)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/admin/rooms"
        );
        let room = crate::repository::room_repository::find_by_id(&pool, room_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(room.room_name, "New");
        assert_eq!(room.quest_text, "New Quest");
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

    #[tokio::test]
    async fn callback_without_signature_returns_unauthorized() {
        let response = app_router(test_pool())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/callback")
                    .body(Body::from(r#"{"events":[]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn callback_with_invalid_signature_returns_unauthorized() {
        let response = app_router(test_pool())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/callback")
                    .header("x-line-signature", "invalid")
                    .body(Body::from(r#"{"events":[]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn admin_ranking_redirects_when_not_logged_in() {
        let response = app_router(test_pool())
            .oneshot(
                Request::builder()
                    .uri("/admin/ranking")
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

    async fn seed_ranking_player(
        pool: &sqlx::MySqlPool,
        event_id: i32,
        line_user_id: &str,
        player_name: &str,
        started_at: &str,
        finished_at: Option<&str>,
    ) {
        let player_id = crate::repository::player_repository::insert(
            pool,
            line_user_id,
            event_id,
            player_name,
            &format!("rank-{line_user_id}"),
        )
        .await
        .unwrap();
        sqlx::query("UPDATE players SET started_at = ?, finished_at = ? WHERE id = ?")
            .bind(started_at)
            .bind(finished_at)
            .bind(player_id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[sqlx::test]
    async fn authenticated_session_can_view_ranked_finished_player(pool: sqlx::MySqlPool) {
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
        seed_ranking_player(
            &pool,
            event_id,
            "line-ranking-finished",
            "Alice",
            "2026-01-01 10:00:00",
            Some("2026-01-01 10:12:34"),
        )
        .await;
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
                    .uri("/admin/ranking")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("Alice"));
        assert!(body.contains("1位"));
        assert!(body.contains("12:34"));
    }

    #[sqlx::test]
    async fn authenticated_session_can_view_unfinished_player_without_rank(pool: sqlx::MySqlPool) {
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
        seed_ranking_player(
            &pool,
            event_id,
            "line-ranking-unfinished",
            "Bob",
            "2026-01-01 10:00:00",
            None,
        )
        .await;
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
                    .uri("/admin/ranking")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        let unfinished_heading = body.find("未クリア").unwrap();
        let bob = body.find("Bob").unwrap();

        assert!(body.contains("圏外"));
        assert!(bob > unfinished_heading);
        assert!(!body.contains("1位"));
    }

    #[tokio::test]
    async fn admin_settings_redirects_when_not_logged_in() {
        let response = app_router(test_pool())
            .oneshot(
                Request::builder()
                    .uri("/admin/settings")
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
    async fn authenticated_session_can_view_settings_form(pool: sqlx::MySqlPool) {
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
        crate::repository::event_repository::update_settings(&pool, event_id, true, false)
            .await
            .unwrap();
        let app = app_router(pool.clone());
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
                    .uri("/admin/settings")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains(r#"name="is_team_mode" checked"#));
        assert!(body.contains(r#"name="require_answer_check""#));
        assert!(!body.contains(r#"name="require_answer_check" checked"#));
    }

    #[sqlx::test]
    async fn post_settings_with_checked_boxes_updates_flags(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app = app_router(pool.clone());
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
                    .method("POST")
                    .uri("/admin/settings")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::from(format!(
                        "csrf_token={csrf_token}&is_team_mode=on&require_answer_check=on"
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        let event = crate::repository::event_repository::find_singleton(&pool)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "/admin/settings"
        );
        assert!(event.is_team_mode);
        assert!(event.require_answer_check);
    }

    #[sqlx::test]
    async fn post_settings_without_checkbox_fields_sets_flags_false(pool: sqlx::MySqlPool) {
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
        crate::repository::event_repository::update_settings(&pool, event_id, true, true)
            .await
            .unwrap();
        let app = app_router(pool.clone());
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
                    .method("POST")
                    .uri("/admin/settings")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .header(header::COOKIE, session_cookie)
                    .body(Body::from(format!("csrf_token={csrf_token}")))
                    .unwrap(),
            )
            .await
            .unwrap();
        let event = crate::repository::event_repository::find_singleton(&pool)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(response.status(), StatusCode::FOUND);
        assert!(!event.is_team_mode);
        assert!(!event.require_answer_check);
    }

    #[sqlx::test]
    async fn post_settings_rejects_invalid_csrf_without_changing_db(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let app = app_router(pool.clone());
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

        for body in [
            "is_team_mode=on&require_answer_check=on".to_string(),
            "csrf_token=wrong&is_team_mode=on&require_answer_check=on".to_string(),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/admin/settings")
                        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                        .header(header::COOKIE, session_cookie.clone())
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            let event = crate::repository::event_repository::find_singleton(&pool)
                .await
                .unwrap()
                .unwrap();

            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            assert!(!event.is_team_mode);
            assert!(!event.require_answer_check);
        }
    }
}
