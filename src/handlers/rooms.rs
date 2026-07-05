use axum::response::IntoResponse;

pub async fn list() -> impl IntoResponse {
    "rooms"
}
