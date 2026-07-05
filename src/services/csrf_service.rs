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

pub async fn verify_token(session: &Session, submitted: &str) -> bool {
    if submitted.is_empty() {
        return false;
    }

    matches!(
        session.get::<String>(CSRF_TOKEN_KEY).await,
        Ok(Some(token)) if token == submitted
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tower_sessions::{MemoryStore, Session};

    use super::{issue_token, verify_token};

    #[tokio::test]
    async fn issues_and_verifies_session_csrf_token() {
        let session = Session::new(None, Arc::new(MemoryStore::default()), None);

        let token = issue_token(&session).await;

        assert!(!token.is_empty());
        assert_eq!(issue_token(&session).await, token);
        assert!(verify_token(&session, &token).await);
        assert!(!verify_token(&session, "").await);
        assert!(!verify_token(&session, "wrong").await);
    }
}
