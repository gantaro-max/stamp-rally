use tower_sessions::Session;
use uuid::Uuid;

const CSRF_TOKEN_KEY: &str = "csrf_token";

pub async fn issue_token(session: &Session) -> String {
    if let Ok(Some(token)) = session.get::<String>(CSRF_TOKEN_KEY).await {
        return token;
    }

    let token = Uuid::new_v4().to_string();
    session
        .insert(CSRF_TOKEN_KEY, token.clone())
        .await
        .expect("session token insertion should not fail");
    token
}
