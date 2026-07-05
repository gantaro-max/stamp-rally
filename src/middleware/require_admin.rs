use axum::{
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tower_sessions::Session;

pub async fn require_admin(_session: Session, _request: Request, _next: Next) -> Response {
    redirect_to("/auth/login")
}

fn redirect_to(location: &'static str) -> Response {
    (StatusCode::FOUND, [(header::LOCATION, location)]).into_response()
}
