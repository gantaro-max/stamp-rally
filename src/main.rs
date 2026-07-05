mod handlers;
mod repository;
mod services;

use axum::{Router, routing::get};
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

fn app_router(pool: MySqlPool) -> Router {
    let session_layer = SessionManagerLayer::new(MemoryStore::default())
        .with_expiry(Expiry::OnInactivity(Duration::hours(12)));

    Router::new()
        .route("/health", get(handlers::health::health))
        .route("/auth/login", get(handlers::auth::login_form))
        .with_state(pool)
        .layer(session_layer)
}

#[cfg(test)]
mod tests {
    use super::app_router;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use sqlx::mysql::MySqlPoolOptions;
    use tower::ServiceExt;

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
}
