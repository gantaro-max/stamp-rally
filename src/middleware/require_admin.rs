use axum::{
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tower_sessions::Session;

pub async fn require_admin(session: Session, request: Request, next: Next) -> Response {
    match session.get::<bool>("admin_authenticated").await {
        Ok(Some(true)) => next.run(request).await,
        _ => redirect_to("/auth/login"),
    }
}

fn redirect_to(location: &'static str) -> Response {
    (StatusCode::FOUND, [(header::LOCATION, location)]).into_response()
}
