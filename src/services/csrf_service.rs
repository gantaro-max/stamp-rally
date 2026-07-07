use tower_sessions::Session;
use uuid::Uuid;

const CSRF_TOKEN_KEY: &str = "csrf_token";

pub async fn issue_token(session: &Session) -> Result<String, tower_sessions::session::Error> {
    if let Some(token) = session.get::<String>(CSRF_TOKEN_KEY).await? {
        return Ok(token);
    }

    let token = Uuid::new_v4().to_string();
    session.insert(CSRF_TOKEN_KEY, token.clone()).await?;
    Ok(token)
}

pub async fn verify_token(session: &Session, submitted: &str) -> bool {
    if submitted.is_empty() {
        return false;
    }

    matches!(
        session.get::<String>(CSRF_TOKEN_KEY).await,
        Ok(Some(token)) if constant_time_eq(token.as_bytes(), submitted.as_bytes())
    )
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    let diff = left
        .iter()
        .zip(right.iter())
        .fold(0_u8, |acc, (left, right)| acc | (left ^ right));
    diff == 0
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tower_sessions::{MemoryStore, Session};

    use super::{issue_token, verify_token};

    #[tokio::test]
    async fn issues_and_verifies_session_csrf_token() {
        let session = Session::new(None, Arc::new(MemoryStore::default()), None);

        let token = issue_token(&session).await.unwrap();

        assert!(!token.is_empty());
        assert_eq!(issue_token(&session).await.unwrap(), token);
        assert!(verify_token(&session, &token).await);
        assert!(!verify_token(&session, "").await);
        assert!(!verify_token(&session, "wrong").await);
    }
}
