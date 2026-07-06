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
