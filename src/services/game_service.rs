#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplyMessage {
    Text(String),
    Quest {
        room_name: String,
        quest_text: String,
        image_url: Option<String>,
    },
}


#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{PendingRegistrations, ReplyMessage};

    const PUBLIC_BASE_URL: &str = "https://example.test";

    fn pending() -> PendingRegistrations {
        Arc::new(Mutex::new(std::collections::HashSet::new()))
    }

    async fn seed_event(pool: &sqlx::MySqlPool) -> i32 {
        crate::services::auth_service::seed_admin_event_if_empty(pool, "admin-secret", "Stamp Rally")
            .await
            .unwrap();
        crate::repository::event_repository::find_singleton(pool)
            .await
            .unwrap()
            .unwrap()
            .id
    }

    async fn set_event_flags(pool: &sqlx::MySqlPool, is_team_mode: bool, require_answer_check: bool) -> i32 {
        let event_id = seed_event(pool).await;
        sqlx::query("UPDATE events SET is_team_mode = ?, require_answer_check = ? WHERE id = ?")
            .bind(is_team_mode)
            .bind(require_answer_check)
            .bind(event_id)
            .execute(pool)
            .await
            .unwrap();
        event_id
    }

    async fn seed_room(pool: &sqlx::MySqlPool, event_id: i32, name: &str, answer: Option<&str>, hint: Option<&str>) -> i32 {
        crate::repository::room_repository::insert(
            pool,
            event_id,
            name,
            "Find the red book",
            answer,
            hint,
            None,
            &format!("qr-{name}"),
        )
        .await
        .unwrap()
    }

    async fn seed_player_with_room(pool: &sqlx::MySqlPool, event_id: i32, line_user_id: &str, require_answer_check: bool) -> (i32, i32) {
        let answer = require_answer_check.then_some("Red, blue");
        let hint = require_answer_check.then_some("Look near the shelf");
        let room_id = seed_room(pool, event_id, "Library", answer, hint).await;
        let player_id = crate::repository::player_repository::insert(pool, line_user_id, event_id, "Alice")
            .await
            .unwrap();
        crate::repository::player_repository::update_current_room(pool, player_id, room_id)
            .await
            .unwrap();
        (player_id, room_id)
    }

    fn text(reply: ReplyMessage) -> String {
        match reply {
            ReplyMessage::Text(value) => value,
            ReplyMessage::Quest { .. } => panic!("expected text reply"),
        }
    }

    fn quest(reply: ReplyMessage) -> (String, String, Option<String>) {
        match reply {
            ReplyMessage::Quest { room_name, quest_text, image_url } => (room_name, quest_text, image_url),
            ReplyMessage::Text(value) => panic!("expected quest reply: {value}"),
        }
    }

    #[sqlx::test]
    async fn starts_registration_for_individual_event(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, false, false).await;
        let pending = pending();

        let reply = super::handle_text_message(&pool, &pending, PUBLIC_BASE_URL, "line-start-individual", "開始").await.unwrap();

        assert!(text(reply).contains("個人名"));
        assert!(pending.lock().unwrap().contains("line-start-individual"));
    }

    #[sqlx::test]
    async fn starts_registration_for_team_event(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, true, false).await;
        let pending = pending();

        let reply = super::handle_text_message(&pool, &pending, PUBLIC_BASE_URL, "line-start-team", "開始").await.unwrap();

        assert!(text(reply).contains("チーム名"));
    }

    #[sqlx::test]
    async fn pending_name_creates_player_assigns_room_and_returns_quest(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_room(&pool, event_id, "Library", None, None).await;
        let pending = pending();
        pending.lock().unwrap().insert("line-name".to_string());

        let reply = super::handle_text_message(&pool, &pending, PUBLIC_BASE_URL, "line-name", "Alice").await.unwrap();
        let (room_name, quest_text, image_url) = quest(reply);
        let player = crate::repository::player_repository::find_by_line_user_and_event(&pool, "line-name", event_id).await.unwrap().unwrap();

        assert_eq!(room_name, "Library");
        assert_eq!(quest_text, "Find the red book");
        assert!(image_url.is_none());
        assert!(player.current_room_id.is_some());
        assert!(!pending.lock().unwrap().contains("line-name"));
    }

    #[sqlx::test]
    async fn pending_blank_name_prompts_again_without_creating_player(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        let pending = pending();
        pending.lock().unwrap().insert("line-blank".to_string());

        let reply = super::handle_text_message(&pool, &pending, PUBLIC_BASE_URL, "line-blank", "   ").await.unwrap();

        assert!(text(reply).contains("入力"));
        assert!(crate::repository::player_repository::find_by_line_user_and_event(&pool, "line-blank", event_id).await.unwrap().is_none());
        assert!(pending.lock().unwrap().contains("line-blank"));
    }

    #[sqlx::test]
    async fn pending_name_without_rooms_returns_error_without_creating_player(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        let pending = pending();
        pending.lock().unwrap().insert("line-no-room".to_string());

        let reply = super::handle_text_message(&pool, &pending, PUBLIC_BASE_URL, "line-no-room", "Alice").await.unwrap();

        assert!(text(reply).contains("部屋が登録されていません"));
        assert!(crate::repository::player_repository::find_by_line_user_and_event(&pool, "line-no-room", event_id).await.unwrap().is_none());
        assert!(!pending.lock().unwrap().contains("line-no-room"));
    }

    #[sqlx::test]
    async fn registered_start_resends_current_room_quest(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        let (_player_id, room_id) = seed_player_with_room(&pool, event_id, "line-registered-start", false).await;

        let reply = super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-registered-start", "開始").await.unwrap();
        let player = crate::repository::player_repository::find_by_line_user_and_event(&pool, "line-registered-start", event_id).await.unwrap().unwrap();

        assert_eq!(quest(reply).0, "Library");
        assert_eq!(player.current_room_id, Some(room_id));
    }

    #[sqlx::test]
    async fn finished_start_returns_finished_message(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        let (player_id, _room_id) = seed_player_with_room(&pool, event_id, "line-finished", false).await;
        sqlx::query("UPDATE players SET finished_at = NOW() WHERE id = ?").bind(player_id).execute(&pool).await.unwrap();

        let reply = super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-finished", "開始").await.unwrap();

        assert!(text(reply).contains("クリア済み"));
    }

    #[sqlx::test]
    async fn registered_reset_deletes_player(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_player_with_room(&pool, event_id, "line-reset", false).await;

        let reply = super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-reset", "リセット").await.unwrap();

        assert!(text(reply).contains("削除"));
        assert!(crate::repository::player_repository::find_by_line_user_and_event(&pool, "line-reset", event_id).await.unwrap().is_none());
    }

    #[sqlx::test]
    async fn unregistered_reset_reports_not_registered(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, false, false).await;

        let reply = super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-reset-missing", "リセット").await.unwrap();

        assert!(text(reply).contains("参加登録されていません"));
    }

    #[sqlx::test]
    async fn help_always_returns_guide(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, false, false).await;
        let pending = pending();
        pending.lock().unwrap().insert("line-help".to_string());

        for command in ["遊び方", "ヘルプ"] {
            let reply = super::handle_text_message(&pool, &pending, PUBLIC_BASE_URL, "line-help", command).await.unwrap();
            assert!(text(reply).contains("開始"));
        }
    }

    #[sqlx::test]
    async fn hint_is_unavailable_without_answer_check(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_player_with_room(&pool, event_id, "line-hint-off", false).await;

        let reply = super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-hint-off", "ヒント").await.unwrap();

        assert!(text(reply).contains("利用できません"));
    }

    #[sqlx::test]
    async fn hint_returns_current_room_hint_when_answer_check_is_required(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, true).await;
        seed_player_with_room(&pool, event_id, "line-hint-on", true).await;

        let reply = super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-hint-on", "ヒント").await.unwrap();

        assert_eq!(text(reply), "Look near the shelf");
    }

    #[sqlx::test]
    async fn hint_reports_missing_hint(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, true).await;
        let room_id = seed_room(&pool, event_id, "Library", Some("Red"), None).await;
        let player_id = crate::repository::player_repository::insert(&pool, "line-hint-missing", event_id, "Alice").await.unwrap();
        crate::repository::player_repository::update_current_room(&pool, player_id, room_id).await.unwrap();

        let reply = super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-hint-missing", "ヒント").await.unwrap();

        assert!(text(reply).contains("登録されていません"));
    }

    #[sqlx::test]
    async fn correct_answer_sets_verified(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, true).await;
        let (player_id, _room_id) = seed_player_with_room(&pool, event_id, "line-correct", true).await;

        let reply = super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-correct", " red ").await.unwrap();
        let player = crate::repository::player_repository::find_by_line_user_and_event(&pool, "line-correct", event_id).await.unwrap().unwrap();

        assert!(text(reply).contains("QR"));
        assert_eq!(player.id, player_id);
        assert!(player.answer_verified);
    }

    #[sqlx::test]
    async fn wrong_answer_keeps_verified_false(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, true).await;
        seed_player_with_room(&pool, event_id, "line-wrong", true).await;

        let reply = super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-wrong", "green").await.unwrap();
        let player = crate::repository::player_repository::find_by_line_user_and_event(&pool, "line-wrong", event_id).await.unwrap().unwrap();

        assert!(text(reply).contains("不正解"));
        assert!(!player.answer_verified);
    }

    #[sqlx::test]
    async fn verified_player_free_text_repeats_qr_message(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, true).await;
        let (player_id, _room_id) = seed_player_with_room(&pool, event_id, "line-verified", true).await;
        crate::repository::player_repository::set_answer_verified(&pool, player_id, true).await.unwrap();

        let reply = super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-verified", "anything").await.unwrap();

        assert!(text(reply).contains("QR"));
    }

    #[sqlx::test]
    async fn free_text_without_answer_check_prompts_qr(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_player_with_room(&pool, event_id, "line-free", false).await;

        let reply = super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-free", "anything").await.unwrap();

        assert!(text(reply).contains("QRコード"));
    }

    #[sqlx::test]
    async fn unregistered_free_text_prompts_start(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, false, false).await;

        let reply = super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-unknown", "hello").await.unwrap();

        assert!(text(reply).contains("開始"));
    }
}
