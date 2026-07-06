use base64::{Engine as _, engine::general_purpose};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

use crate::services::game_service::ReplyMessage;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug)]
pub enum LineClientError {
    Request(reqwest::Error),
    ApiStatus(reqwest::StatusCode),
}

impl From<reqwest::Error> for LineClientError {
    fn from(err: reqwest::Error) -> Self {
        Self::Request(err)
    }
}

pub fn verify_signature(channel_secret: &str, body: &[u8], signature_header: &str) -> bool {
    if signature_header.is_empty() {
        return false;
    }

    let Ok(mut mac) = HmacSha256::new_from_slice(channel_secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    let expected = mac.finalize().into_bytes();
    let Ok(actual) = general_purpose::STANDARD.decode(signature_header) else {
        return false;
    };

    constant_time_eq(expected.as_slice(), &actual)
}

fn constant_time_eq(expected: &[u8], actual: &[u8]) -> bool {
    if expected.len() != actual.len() {
        return false;
    }

    let diff = expected
        .iter()
        .zip(actual.iter())
        .fold(0u8, |diff, (left, right)| diff | (left ^ right));
    diff == 0
}

pub fn build_text_message(text: &str) -> Value {
    json!({"type": "text", "text": text})
}

pub fn build_quest_flex_message(room_name: &str, quest_text: &str, image_url: Option<&str>) -> Value {
    let mut contents = json!({
        "type": "bubble",
        "body": {
            "type": "box",
            "layout": "vertical",
            "contents": [
                {"type": "text", "text": room_name, "weight": "bold", "wrap": true},
                {"type": "text", "text": quest_text, "wrap": true}
            ]
        }
    });

    if let Some(url) = image_url {
        contents["hero"] = json!({
            "type": "image",
            "url": url,
            "size": "full",
            "aspectRatio": "20:13",
            "aspectMode": "cover"
        });
    }

    json!({
        "type": "flex",
        "altText": format!("{room_name} のクエスト"),
        "contents": contents
    })
}

pub fn to_line_message(reply: &ReplyMessage) -> Value {
    match reply {
        ReplyMessage::Text(text) => build_text_message(text),
        ReplyMessage::Quest {
            room_name,
            quest_text,
            image_url,
        } => build_quest_flex_message(room_name, quest_text, image_url.as_deref()),
    }
}

pub async fn send_reply(
    client: &reqwest::Client,
    access_token: &str,
    reply_token: &str,
    message: Value,
) -> Result<(), LineClientError> {
    // Integration tests do not exercise LINE's network API in this environment.
    let response = client
        .post("https://api.line.me/v2/bot/message/reply")
        .bearer_auth(access_token)
        .json(&json!({"replyToken": reply_token, "messages": [message]}))
        .send()
        .await?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(LineClientError::ApiStatus(response.status()))
    }
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose};
    use hmac::{Hmac, Mac};
    use serde_json::json;
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    fn signature(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    #[test]
    fn verify_signature_accepts_valid_signature() {
        let secret = "channel-secret";
        let body = br#"{"events":[]}"#;
        let signature = signature(secret, body);

        assert!(super::verify_signature(secret, body, &signature));
    }

    #[test]
    fn verify_signature_rejects_wrong_secret_or_tampered_body() {
        let secret = "channel-secret";
        let body = br#"{"events":[]}"#;
        let signature = signature(secret, body);

        assert!(!super::verify_signature("wrong-secret", body, &signature));
        assert!(!super::verify_signature(
            secret,
            br#"{"events":[{"type":"message"}]}"#,
            &signature
        ));
    }

    #[test]
    fn verify_signature_rejects_empty_header() {
        assert!(!super::verify_signature("channel-secret", b"body", ""));
    }

    #[test]
    fn build_text_message_returns_line_text_json() {
        assert_eq!(
            super::build_text_message("hello"),
            json!({"type": "text", "text": "hello"})
        );
    }

    #[test]
    fn build_quest_flex_message_includes_hero_when_image_url_is_present() {
        let message = super::build_quest_flex_message(
            "Library",
            "Find the red book",
            Some("https://example.test/public/image/image-uuid"),
        );

        assert_eq!(message["type"], "flex");
        assert_eq!(message["altText"], "Library のクエスト");
        assert_eq!(
            message["contents"]["hero"]["url"],
            "https://example.test/public/image/image-uuid"
        );
        assert_eq!(message["contents"]["body"]["contents"][0]["text"], "Library");
        assert_eq!(
            message["contents"]["body"]["contents"][1]["text"],
            "Find the red book"
        );
    }

    #[test]
    fn build_quest_flex_message_omits_hero_when_image_url_is_absent() {
        let message = super::build_quest_flex_message("Library", "Find the red book", None);

        assert_eq!(message["type"], "flex");
        assert!(message["contents"].get("hero").is_none());
        assert_eq!(message["altText"], "Library のクエスト");
    }
}
