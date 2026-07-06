use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use sqlx::MySqlPool;

use crate::repository::{
    event_repository, player_repository, room_image_repository, room_repository,
};

pub type PendingRegistrations = Arc<Mutex<HashSet<String>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplyMessage {
    Text(String),
    Quest {
        room_name: String,
        quest_text: String,
        image_url: Option<String>,
    },
}

#[derive(Debug)]
pub enum GameServiceError {
    Database(sqlx::Error),
}

impl From<sqlx::Error> for GameServiceError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err)
    }
}

impl std::fmt::Display for GameServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(err) => write!(f, "database error: {err}"),
        }
    }
}

impl std::error::Error for GameServiceError {}

pub async fn handle_text_message(
    pool: &MySqlPool,
    pending: &PendingRegistrations,
    public_base_url: &str,
    line_user_id: &str,
    text: &str,
) -> Result<ReplyMessage, GameServiceError> {
    let text = text.trim();
    let Some(event) = event_repository::find_singleton(pool).await? else {
        return Ok(ReplyMessage::Text(
            "イベントが設定されていません。管理者にお問い合わせください。".to_string(),
        ));
    };
    let player =
        player_repository::find_by_line_user_and_event(pool, line_user_id, event.id).await?;

    if matches!(text, "遊び方" | "ヘルプ") {
        return Ok(ReplyMessage::Text(help_text()));
    }

    if text == "リセット" {
        let had_pending = was_pending_removed(pending, line_user_id);
        if player.is_some() {
            player_repository::delete_by_line_user_and_event(pool, line_user_id, event.id).await?;
            return Ok(ReplyMessage::Text(
                "参加データを削除しました。もう一度参加する場合は『開始』と送信してください。"
                    .to_string(),
            ));
        }
        return if had_pending {
            Ok(ReplyMessage::Text("登録をキャンセルしました。".to_string()))
        } else {
            Ok(ReplyMessage::Text(
                "現在参加登録されていません。".to_string(),
            ))
        };
    }

    if text == "開始" {
        if let Some(player) = player.as_ref() {
            if player.finished_at.is_some() {
                return Ok(ReplyMessage::Text(
                    "クリア済みです。最初の部屋に戻ってください。".to_string(),
                ));
            }
            return quest_reply_for_player(pool, public_base_url, player).await;
        }
        if is_pending(pending, line_user_id) {
            return Ok(ReplyMessage::Text(name_prompt(event.is_team_mode)));
        }
        add_pending(pending, line_user_id);
        return Ok(ReplyMessage::Text(name_prompt(event.is_team_mode)));
    }

    if player.is_none() && is_pending(pending, line_user_id) {
        if text.is_empty() {
            return Ok(ReplyMessage::Text(name_prompt(event.is_team_mode)));
        }
        if room_repository::count(pool, event.id).await? == 0 {
            remove_pending(pending, line_user_id);
            return Ok(ReplyMessage::Text(
                "参加できる部屋が登録されていません。管理者にお問い合わせください。".to_string(),
            ));
        }
        let player_id = player_repository::insert(pool, line_user_id, event.id, text).await?;
        remove_pending(pending, line_user_id);
        let Some(room) = room_repository::find_random_unvisited(pool, event.id, player_id).await?
        else {
            return Ok(ReplyMessage::Text(
                "参加できる部屋が登録されていません。管理者にお問い合わせください。".to_string(),
            ));
        };
        player_repository::update_current_room(pool, player_id, room.id).await?;
        return quest_reply_for_room(pool, public_base_url, &room).await;
    }

    let Some(player) = player else {
        return Ok(ReplyMessage::Text(
            "『開始』と送信して参加登録してください。".to_string(),
        ));
    };

    if player.finished_at.is_some() {
        return Ok(ReplyMessage::Text(
            "クリア済みです。最初の部屋に戻ってください。".to_string(),
        ));
    }

    if text == "ヒント" {
        if !event.require_answer_check {
            return Ok(ReplyMessage::Text(
                "このイベントではヒント機能は利用できません。".to_string(),
            ));
        }
        let Some(room) = current_room(pool, &player).await? else {
            return Ok(ReplyMessage::Text(
                "現在の部屋が設定されていません。『開始』と送信してください。".to_string(),
            ));
        };
        return Ok(ReplyMessage::Text(
            room.hint_msg
                .unwrap_or_else(|| "ヒントは登録されていません。".to_string()),
        ));
    }

    if !event.require_answer_check {
        return Ok(ReplyMessage::Text(
            "QRコードを読み込んでください。".to_string(),
        ));
    }
    if player.answer_verified {
        return Ok(ReplyMessage::Text(
            "正解済みです。QRコードを読み込んでください。".to_string(),
        ));
    }

    let Some(room) = current_room(pool, &player).await? else {
        return Ok(ReplyMessage::Text(
            "現在の部屋が設定されていません。『開始』と送信してください。".to_string(),
        ));
    };
    if is_correct_answer(room.answer.as_deref(), text) {
        player_repository::set_answer_verified(pool, player.id, true).await?;
        Ok(ReplyMessage::Text(
            "正解です！QRコードを読み込んでください。".to_string(),
        ))
    } else {
        Ok(ReplyMessage::Text(
            "不正解です。もう一度お試しください。".to_string(),
        ))
    }
}

fn help_text() -> String {
    "『開始』で参加登録します。案内された部屋へ移動し、必要に応じて答えを送信してからQRコードを読み込んでください。『ヒント』でヒント、『リセット』で参加データを削除できます。".to_string()
}

fn name_prompt(is_team_mode: bool) -> String {
    if is_team_mode {
        "チーム名を入力してください。".to_string()
    } else {
        "個人名を入力してください。".to_string()
    }
}

fn is_pending(pending: &PendingRegistrations, line_user_id: &str) -> bool {
    pending.lock().unwrap().contains(line_user_id)
}

fn add_pending(pending: &PendingRegistrations, line_user_id: &str) {
    pending.lock().unwrap().insert(line_user_id.to_string());
}

fn remove_pending(pending: &PendingRegistrations, line_user_id: &str) {
    pending.lock().unwrap().remove(line_user_id);
}

fn was_pending_removed(pending: &PendingRegistrations, line_user_id: &str) -> bool {
    pending.lock().unwrap().remove(line_user_id)
}

async fn quest_reply_for_player(
    pool: &MySqlPool,
    public_base_url: &str,
    player: &player_repository::Player,
) -> Result<ReplyMessage, GameServiceError> {
    let Some(room) = current_room(pool, player).await? else {
        return Ok(ReplyMessage::Text(
            "現在の部屋が設定されていません。『開始』と送信してください。".to_string(),
        ));
    };
    quest_reply_for_room(pool, public_base_url, &room).await
}

async fn current_room(
    pool: &MySqlPool,
    player: &player_repository::Player,
) -> Result<Option<room_repository::Room>, sqlx::Error> {
    let Some(room_id) = player.current_room_id else {
        return Ok(None);
    };
    room_repository::find_by_id(pool, room_id).await
}

async fn quest_reply_for_room(
    pool: &MySqlPool,
    public_base_url: &str,
    room: &room_repository::Room,
) -> Result<ReplyMessage, GameServiceError> {
    let image_url = if let Some(image_id) = room.image_id {
        room_image_repository::find_uuid_by_id(pool, image_id)
            .await?
            .map(|uuid| {
                format!(
                    "{}/public/image/{uuid}",
                    public_base_url.trim_end_matches('/')
                )
            })
    } else {
        None
    };

    Ok(ReplyMessage::Quest {
        room_name: room.room_name.clone(),
        quest_text: room.quest_text.clone(),
        image_url,
    })
}

fn is_correct_answer(answer: Option<&str>, text: &str) -> bool {
    let normalized = text.trim().to_lowercase();
    answer
        .unwrap_or_default()
        .split(',')
        .any(|candidate| candidate.trim().to_lowercase() == normalized)
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
        crate::services::auth_service::seed_admin_event_if_empty(
            pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        crate::repository::event_repository::find_singleton(pool)
            .await
            .unwrap()
            .unwrap()
            .id
    }

    async fn set_event_flags(
        pool: &sqlx::MySqlPool,
        is_team_mode: bool,
        require_answer_check: bool,
    ) -> i32 {
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

    async fn seed_room(
        pool: &sqlx::MySqlPool,
        event_id: i32,
        name: &str,
        answer: Option<&str>,
        hint: Option<&str>,
    ) -> i32 {
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

    async fn seed_player_with_room(
        pool: &sqlx::MySqlPool,
        event_id: i32,
        line_user_id: &str,
        require_answer_check: bool,
    ) -> (i32, i32) {
        let answer = require_answer_check.then_some("Red, blue");
        let hint = require_answer_check.then_some("Look near the shelf");
        let room_id = seed_room(pool, event_id, "Library", answer, hint).await;
        let player_id =
            crate::repository::player_repository::insert(pool, line_user_id, event_id, "Alice")
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
            ReplyMessage::Quest {
                room_name,
                quest_text,
                image_url,
            } => (room_name, quest_text, image_url),
            ReplyMessage::Text(value) => panic!("expected quest reply: {value}"),
        }
    }

    #[sqlx::test]
    async fn starts_registration_for_individual_event(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, false, false).await;
        let pending = pending();

        let reply = super::handle_text_message(
            &pool,
            &pending,
            PUBLIC_BASE_URL,
            "line-start-individual",
            "開始",
        )
        .await
        .unwrap();

        assert!(text(reply).contains("個人名"));
        assert!(pending.lock().unwrap().contains("line-start-individual"));
    }

    #[sqlx::test]
    async fn starts_registration_for_team_event(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, true, false).await;
        let pending = pending();

        let reply =
            super::handle_text_message(&pool, &pending, PUBLIC_BASE_URL, "line-start-team", "開始")
                .await
                .unwrap();

        assert!(text(reply).contains("チーム名"));
    }

    #[sqlx::test]
    async fn pending_name_creates_player_assigns_room_and_returns_quest(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_room(&pool, event_id, "Library", None, None).await;
        let pending = pending();
        pending.lock().unwrap().insert("line-name".to_string());

        let reply =
            super::handle_text_message(&pool, &pending, PUBLIC_BASE_URL, "line-name", "Alice")
                .await
                .unwrap();
        let (room_name, quest_text, image_url) = quest(reply);
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-name",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

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

        let reply =
            super::handle_text_message(&pool, &pending, PUBLIC_BASE_URL, "line-blank", "   ")
                .await
                .unwrap();

        assert!(text(reply).contains("入力"));
        assert!(
            crate::repository::player_repository::find_by_line_user_and_event(
                &pool,
                "line-blank",
                event_id
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(pending.lock().unwrap().contains("line-blank"));
    }

    #[sqlx::test]
    async fn pending_name_without_rooms_returns_error_without_creating_player(
        pool: sqlx::MySqlPool,
    ) {
        let event_id = set_event_flags(&pool, false, false).await;
        let pending = pending();
        pending.lock().unwrap().insert("line-no-room".to_string());

        let reply =
            super::handle_text_message(&pool, &pending, PUBLIC_BASE_URL, "line-no-room", "Alice")
                .await
                .unwrap();

        assert!(text(reply).contains("部屋が登録されていません"));
        assert!(
            crate::repository::player_repository::find_by_line_user_and_event(
                &pool,
                "line-no-room",
                event_id
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(!pending.lock().unwrap().contains("line-no-room"));
    }

    #[sqlx::test]
    async fn registered_start_resends_current_room_quest(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        let (_player_id, room_id) =
            seed_player_with_room(&pool, event_id, "line-registered-start", false).await;

        let reply = super::handle_text_message(
            &pool,
            &pending(),
            PUBLIC_BASE_URL,
            "line-registered-start",
            "開始",
        )
        .await
        .unwrap();
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-registered-start",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(quest(reply).0, "Library");
        assert_eq!(player.current_room_id, Some(room_id));
    }

    #[sqlx::test]
    async fn finished_start_returns_finished_message(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        let (player_id, _room_id) =
            seed_player_with_room(&pool, event_id, "line-finished", false).await;
        sqlx::query("UPDATE players SET finished_at = NOW() WHERE id = ?")
            .bind(player_id)
            .execute(&pool)
            .await
            .unwrap();

        let reply =
            super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-finished", "開始")
                .await
                .unwrap();

        assert!(text(reply).contains("クリア済み"));
    }

    #[sqlx::test]
    async fn registered_reset_deletes_player(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_player_with_room(&pool, event_id, "line-reset", false).await;

        let reply = super::handle_text_message(
            &pool,
            &pending(),
            PUBLIC_BASE_URL,
            "line-reset",
            "リセット",
        )
        .await
        .unwrap();

        assert!(text(reply).contains("削除"));
        assert!(
            crate::repository::player_repository::find_by_line_user_and_event(
                &pool,
                "line-reset",
                event_id
            )
            .await
            .unwrap()
            .is_none()
        );
    }

    #[sqlx::test]
    async fn unregistered_reset_reports_not_registered(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, false, false).await;

        let reply = super::handle_text_message(
            &pool,
            &pending(),
            PUBLIC_BASE_URL,
            "line-reset-missing",
            "リセット",
        )
        .await
        .unwrap();

        assert!(text(reply).contains("参加登録されていません"));
    }

    #[sqlx::test]
    async fn help_always_returns_guide(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, false, false).await;
        let pending = pending();
        pending.lock().unwrap().insert("line-help".to_string());

        for command in ["遊び方", "ヘルプ"] {
            let reply =
                super::handle_text_message(&pool, &pending, PUBLIC_BASE_URL, "line-help", command)
                    .await
                    .unwrap();
            assert!(text(reply).contains("開始"));
        }
    }

    #[sqlx::test]
    async fn hint_is_unavailable_without_answer_check(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_player_with_room(&pool, event_id, "line-hint-off", false).await;

        let reply = super::handle_text_message(
            &pool,
            &pending(),
            PUBLIC_BASE_URL,
            "line-hint-off",
            "ヒント",
        )
        .await
        .unwrap();

        assert!(text(reply).contains("利用できません"));
    }

    #[sqlx::test]
    async fn hint_returns_current_room_hint_when_answer_check_is_required(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, true).await;
        seed_player_with_room(&pool, event_id, "line-hint-on", true).await;

        let reply = super::handle_text_message(
            &pool,
            &pending(),
            PUBLIC_BASE_URL,
            "line-hint-on",
            "ヒント",
        )
        .await
        .unwrap();

        assert_eq!(text(reply), "Look near the shelf");
    }

    #[sqlx::test]
    async fn hint_reports_missing_hint(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, true).await;
        let room_id = seed_room(&pool, event_id, "Library", Some("Red"), None).await;
        let player_id = crate::repository::player_repository::insert(
            &pool,
            "line-hint-missing",
            event_id,
            "Alice",
        )
        .await
        .unwrap();
        crate::repository::player_repository::update_current_room(&pool, player_id, room_id)
            .await
            .unwrap();

        let reply = super::handle_text_message(
            &pool,
            &pending(),
            PUBLIC_BASE_URL,
            "line-hint-missing",
            "ヒント",
        )
        .await
        .unwrap();

        assert!(text(reply).contains("登録されていません"));
    }

    #[sqlx::test]
    async fn correct_answer_sets_verified(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, true).await;
        let (player_id, _room_id) =
            seed_player_with_room(&pool, event_id, "line-correct", true).await;

        let reply =
            super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-correct", " red ")
                .await
                .unwrap();
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-correct",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        assert!(text(reply).contains("QR"));
        assert_eq!(player.id, player_id);
        assert!(player.answer_verified);
    }

    #[sqlx::test]
    async fn wrong_answer_keeps_verified_false(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, true).await;
        seed_player_with_room(&pool, event_id, "line-wrong", true).await;

        let reply =
            super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-wrong", "green")
                .await
                .unwrap();
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-wrong",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        assert!(text(reply).contains("不正解"));
        assert!(!player.answer_verified);
    }

    #[sqlx::test]
    async fn verified_player_free_text_repeats_qr_message(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, true).await;
        let (player_id, _room_id) =
            seed_player_with_room(&pool, event_id, "line-verified", true).await;
        crate::repository::player_repository::set_answer_verified(&pool, player_id, true)
            .await
            .unwrap();

        let reply = super::handle_text_message(
            &pool,
            &pending(),
            PUBLIC_BASE_URL,
            "line-verified",
            "anything",
        )
        .await
        .unwrap();

        assert!(text(reply).contains("QR"));
    }

    #[sqlx::test]
    async fn free_text_without_answer_check_prompts_qr(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_player_with_room(&pool, event_id, "line-free", false).await;

        let reply =
            super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-free", "anything")
                .await
                .unwrap();

        assert!(text(reply).contains("QRコード"));
    }

    #[sqlx::test]
    async fn unregistered_free_text_prompts_start(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, false, false).await;

        let reply =
            super::handle_text_message(&pool, &pending(), PUBLIC_BASE_URL, "line-unknown", "hello")
                .await
                .unwrap();

        assert!(text(reply).contains("開始"));
    }

    async fn set_require_answer_check(pool: &sqlx::MySqlPool, event_id: i32, enabled: bool) {
        sqlx::query("UPDATE events SET require_answer_check = ? WHERE id = ?")
            .bind(enabled)
            .bind(event_id)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn seed_named_room(
        pool: &sqlx::MySqlPool,
        event_id: i32,
        name: &str,
        qr_uuid: &str,
    ) -> i32 {
        crate::repository::room_repository::insert(
            pool,
            event_id,
            name,
            &format!("Quest for {name}"),
            Some("red"),
            Some("hint"),
            None,
            qr_uuid,
        )
        .await
        .unwrap()
    }

    async fn seed_player_current_room(
        pool: &sqlx::MySqlPool,
        event_id: i32,
        line_user_id: &str,
        room_id: i32,
    ) -> i32 {
        let player_id = crate::repository::player_repository::insert(
            pool,
            line_user_id,
            event_id,
            "Alice",
        )
        .await
        .unwrap();
        crate::repository::player_repository::update_current_room(pool, player_id, room_id)
            .await
            .unwrap();
        player_id
    }

    #[sqlx::test]
    async fn checkin_records_visit_and_returns_next_quest(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let current_room = seed_named_room(&pool, event_id, "Library", "qr-current").await;
        let next_room = seed_named_room(&pool, event_id, "Gym", "qr-next").await;
        let player_id = seed_player_current_room(&pool, event_id, "line-checkin-next", current_room).await;
        crate::repository::player_repository::set_answer_verified(&pool, player_id, true)
            .await
            .unwrap();

        let outcome = super::checkin(&pool, PUBLIC_BASE_URL, "line-checkin-next", "qr-current")
            .await
            .unwrap();
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-checkin-next",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(crate::repository::player_repository::count_visited(&pool, player_id).await.unwrap(), 1);
        assert_eq!(player.current_room_id, Some(next_room));
        assert!(!player.answer_verified);
        assert!(matches!(outcome, super::CheckinOutcome::NextQuest(ReplyMessage::Quest { .. })));
    }

    #[sqlx::test]
    async fn checkin_last_room_marks_finished_and_returns_cleared(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let only_room = seed_named_room(&pool, event_id, "Library", "qr-last").await;
        let player_id = seed_player_current_room(&pool, event_id, "line-checkin-clear", only_room).await;

        let outcome = super::checkin(&pool, PUBLIC_BASE_URL, "line-checkin-clear", "qr-last")
            .await
            .unwrap();
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-checkin-clear",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        assert!(matches!(outcome, super::CheckinOutcome::Cleared));
        assert_eq!(crate::repository::player_repository::count_visited(&pool, player_id).await.unwrap(), 1);
        assert!(player.finished_at.is_some());
    }

    #[sqlx::test]
    async fn checkin_rejects_missing_room_without_db_change(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let current_room = seed_named_room(&pool, event_id, "Library", "qr-existing").await;
        let player_id = seed_player_current_room(&pool, event_id, "line-missing-room", current_room).await;

        let outcome = super::checkin(&pool, PUBLIC_BASE_URL, "line-missing-room", "missing-qr")
            .await
            .unwrap();

        assert_eq!(outcome, super::CheckinOutcome::Rejected(super::CheckinRejection::RoomNotFound));
        assert_eq!(crate::repository::player_repository::count_visited(&pool, player_id).await.unwrap(), 0);
    }

    #[sqlx::test]
    async fn checkin_rejects_unregistered_player(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        seed_named_room(&pool, event_id, "Library", "qr-unregistered").await;

        let outcome = super::checkin(&pool, PUBLIC_BASE_URL, "line-not-registered", "qr-unregistered")
            .await
            .unwrap();

        assert_eq!(outcome, super::CheckinOutcome::Rejected(super::CheckinRejection::NotRegistered));
    }

    #[sqlx::test]
    async fn checkin_rejects_wrong_room_without_visit(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let current_room = seed_named_room(&pool, event_id, "Library", "qr-current-wrong").await;
        seed_named_room(&pool, event_id, "Gym", "qr-wrong").await;
        let player_id = seed_player_current_room(&pool, event_id, "line-wrong-room", current_room).await;

        let outcome = super::checkin(&pool, PUBLIC_BASE_URL, "line-wrong-room", "qr-wrong")
            .await
            .unwrap();

        assert_eq!(outcome, super::CheckinOutcome::Rejected(super::CheckinRejection::WrongRoom));
        assert_eq!(crate::repository::player_repository::count_visited(&pool, player_id).await.unwrap(), 0);
    }

    #[sqlx::test]
    async fn checkin_rejects_when_answer_not_verified(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        set_require_answer_check(&pool, event_id, true).await;
        let room = seed_named_room(&pool, event_id, "Library", "qr-answer-required").await;
        let player_id = seed_player_current_room(&pool, event_id, "line-answer-not-verified", room).await;

        let outcome = super::checkin(&pool, PUBLIC_BASE_URL, "line-answer-not-verified", "qr-answer-required")
            .await
            .unwrap();

        assert_eq!(outcome, super::CheckinOutcome::Rejected(super::CheckinRejection::AnswerNotVerified));
        assert_eq!(crate::repository::player_repository::count_visited(&pool, player_id).await.unwrap(), 0);
    }

    #[sqlx::test]
    async fn checkin_allows_verified_answer_mode(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        set_require_answer_check(&pool, event_id, true).await;
        let room = seed_named_room(&pool, event_id, "Library", "qr-answer-verified").await;
        let player_id = seed_player_current_room(&pool, event_id, "line-answer-verified", room).await;
        crate::repository::player_repository::set_answer_verified(&pool, player_id, true)
            .await
            .unwrap();

        let outcome = super::checkin(&pool, PUBLIC_BASE_URL, "line-answer-verified", "qr-answer-verified")
            .await
            .unwrap();

        assert!(matches!(outcome, super::CheckinOutcome::Cleared));
        assert_eq!(crate::repository::player_repository::count_visited(&pool, player_id).await.unwrap(), 1);
    }

    #[sqlx::test]
    async fn checkin_rejects_already_finished_player(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room = seed_named_room(&pool, event_id, "Library", "qr-finished").await;
        let player_id = seed_player_current_room(&pool, event_id, "line-already-finished", room).await;
        crate::repository::player_repository::mark_finished(&pool, player_id)
            .await
            .unwrap();
        sqlx::query("UPDATE players SET current_room_id = NULL WHERE id = ?")
            .bind(player_id)
            .execute(&pool)
            .await
            .unwrap();

        let outcome = super::checkin(&pool, PUBLIC_BASE_URL, "line-already-finished", "qr-finished")
            .await
            .unwrap();

        assert_eq!(outcome, super::CheckinOutcome::Rejected(super::CheckinRejection::AlreadyFinished));
    }

}