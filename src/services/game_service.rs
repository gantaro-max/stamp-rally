use std::{future::Future, time::Duration};

use sqlx::MySqlPool;
use uuid::Uuid;

use crate::repository::{
    event_repository, pending_registration_repository, player_repository, room_image_repository,
    room_repository,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplyMessage {
    Text(String),
    Quest {
        intro: String,
        room_name: String,
        quest_text: String,
        image_url: Option<String>,
        stamp_card_url: String,
    },
    StampStatus {
        image_url: String,
    },
    Cleared {
        elapsed: String,
        stamp_card_url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckinRejection {
    RoomNotFound,
    NotRegistered,
    AlreadyFinished,
    WrongRoom,
    AnswerNotVerified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckinOutcome {
    NextQuest(ReplyMessage),
    Cleared(ReplyMessage),
    Rejected(CheckinRejection),
}

#[derive(Debug)]
pub enum GameServiceError {
    Database(sqlx::Error),
    Timeout,
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
            Self::Timeout => write!(f, "operation timed out"),
        }
    }
}

impl std::error::Error for GameServiceError {}

pub(crate) const DB_CALL_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) async fn with_db_call_timeout<T>(
    operation: impl Future<Output = Result<T, GameServiceError>>,
) -> Result<T, GameServiceError> {
    tokio::time::timeout(DB_CALL_TIMEOUT, operation)
        .await
        .map_err(|_| GameServiceError::Timeout)?
}

#[cfg(test)]
mod timeout_tests {
    use std::future;

    use super::{GameServiceError, with_db_call_timeout};

    #[tokio::test]
    async fn db_call_timeout_preserves_completed_results() {
        let success = with_db_call_timeout(async { Ok::<_, GameServiceError>("completed") })
            .await
            .unwrap();
        assert_eq!(success, "completed");

        let error = with_db_call_timeout(async {
            Err::<(), _>(GameServiceError::Database(sqlx::Error::RowNotFound))
        })
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            GameServiceError::Database(sqlx::Error::RowNotFound)
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn db_call_timeout_maps_elapsed_to_timeout_error() {
        let call = tokio::spawn(with_db_call_timeout(future::pending::<
            Result<(), GameServiceError>,
        >()));

        tokio::time::advance(super::DB_CALL_TIMEOUT).await;

        assert!(matches!(
            call.await.unwrap(),
            Err(GameServiceError::Timeout)
        ));
    }
}

pub async fn handle_text_message(
    pool: &MySqlPool,
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
        let had_pending =
            pending_registration_repository::delete(pool, line_user_id, event.id).await?;
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
        if pending_registration_repository::exists(pool, line_user_id, event.id).await? {
            return Ok(ReplyMessage::Text(name_prompt(event.is_team_mode)));
        }
        pending_registration_repository::insert(pool, line_user_id, event.id).await?;
        return Ok(ReplyMessage::Text(name_prompt(event.is_team_mode)));
    }

    if player.is_none()
        && pending_registration_repository::exists(pool, line_user_id, event.id).await?
    {
        if text.is_empty() {
            return Ok(ReplyMessage::Text(name_prompt(event.is_team_mode)));
        }
        if room_repository::count(pool, event.id).await? == 0 {
            pending_registration_repository::delete(pool, line_user_id, event.id).await?;
            return Ok(ReplyMessage::Text(
                "参加できる部屋が登録されていません。管理者にお問い合わせください。".to_string(),
            ));
        }
        let stamp_card_token = Uuid::new_v4().to_string();
        let player_id =
            player_repository::insert(pool, line_user_id, event.id, text, &stamp_card_token)
                .await?;
        pending_registration_repository::delete(pool, line_user_id, event.id).await?;
        let Some(room) = room_repository::find_random_unvisited(pool, event.id, player_id).await?
        else {
            return Ok(ReplyMessage::Text(
                "参加できる部屋が登録されていません。管理者にお問い合わせください。".to_string(),
            ));
        };
        player_repository::update_current_room(pool, player_id, room.id).await?;
        return quest_reply_for_room(
            pool,
            public_base_url,
            &room,
            "最初の部屋は",
            &stamp_card_token,
        )
        .await;
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

    if text == "スタンプ状況" {
        return Ok(ReplyMessage::StampStatus {
            image_url: stamp_card_url(public_base_url, &player.stamp_card_token),
        });
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

pub async fn checkin(
    pool: &MySqlPool,
    public_base_url: &str,
    line_user_id: &str,
    room_qr_uuid: &str,
) -> Result<CheckinOutcome, GameServiceError> {
    let Some(room) = room_repository::find_by_qr_uuid(pool, room_qr_uuid).await? else {
        return Ok(CheckinOutcome::Rejected(CheckinRejection::RoomNotFound));
    };
    let Some(event) = event_repository::find_singleton(pool).await? else {
        return Ok(CheckinOutcome::Rejected(CheckinRejection::NotRegistered));
    };
    let Some(player) =
        player_repository::find_by_line_user_and_event(pool, line_user_id, event.id).await?
    else {
        return Ok(CheckinOutcome::Rejected(CheckinRejection::NotRegistered));
    };

    if player.finished_at.is_some() {
        return Ok(CheckinOutcome::Rejected(CheckinRejection::AlreadyFinished));
    }
    if player.current_room_id != Some(room.id) {
        return Ok(CheckinOutcome::Rejected(CheckinRejection::WrongRoom));
    }
    if event.require_answer_check && !player.answer_verified {
        return Ok(CheckinOutcome::Rejected(
            CheckinRejection::AnswerNotVerified,
        ));
    }

    player_repository::insert_visited_room(pool, player.id, room.id).await?;
    let visited_count = player_repository::count_visited(pool, player.id).await?;
    let room_count = room_repository::count(pool, event.id).await?;
    if visited_count >= room_count {
        player_repository::mark_finished(pool, player.id).await?;
        return Ok(CheckinOutcome::Cleared(cleared_reply(
            public_base_url,
            &player,
        )));
    }

    let Some(next_room) = room_repository::find_random_unvisited(pool, event.id, player.id).await?
    else {
        player_repository::mark_finished(pool, player.id).await?;
        return Ok(CheckinOutcome::Cleared(cleared_reply(
            public_base_url,
            &player,
        )));
    };
    player_repository::update_current_room(pool, player.id, next_room.id).await?;
    let intro = format!(
        "【{}】クリアおめでとうございます。次の部屋は",
        room.room_name
    );
    let reply = quest_reply_for_room(
        pool,
        public_base_url,
        &next_room,
        intro,
        &player.stamp_card_token,
    )
    .await?;
    Ok(CheckinOutcome::NextQuest(reply))
}

fn cleared_reply(public_base_url: &str, player: &player_repository::Player) -> ReplyMessage {
    let elapsed = crate::services::ranking_service::format_elapsed(
        chrono::Utc::now().naive_utc() - player.started_at,
    );
    ReplyMessage::Cleared {
        elapsed,
        stamp_card_url: stamp_card_url(public_base_url, &player.stamp_card_token),
    }
}

fn stamp_card_url(public_base_url: &str, token: &str) -> String {
    format!(
        "{}/public/stamp-card/{token}",
        public_base_url.trim_end_matches('/')
    )
}

fn help_text() -> String {
    "『開始』で参加登録します。案内された部屋へ移動し、必要に応じて答えを送信してからQRコードを読み込んでください。『ヒント』でヒント、『スタンプ状況』で現在集めたスタンプを確認、『リセット』で参加データを削除できます。".to_string()
}

fn name_prompt(is_team_mode: bool) -> String {
    if is_team_mode {
        "チーム名を入力してください。".to_string()
    } else {
        "個人名を入力してください。".to_string()
    }
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
    quest_reply_for_room(
        pool,
        public_base_url,
        &room,
        "現在向かっている部屋は",
        &player.stamp_card_token,
    )
    .await
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
    intro: impl Into<String>,
    stamp_card_token: &str,
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
        intro: intro.into(),
        room_name: room.room_name.clone(),
        quest_text: room.quest_text.clone(),
        image_url,
        stamp_card_url: stamp_card_url(public_base_url, stamp_card_token),
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
    use super::ReplyMessage;
    use crate::repository::pending_registration_repository;
    use sqlx::Row;

    const PUBLIC_BASE_URL: &str = "https://example.test";

    async fn insert_pending(pool: &sqlx::MySqlPool, line_user_id: &str, event_id: i32) {
        pending_registration_repository::insert(pool, line_user_id, event_id)
            .await
            .unwrap();
    }

    async fn pending_exists(pool: &sqlx::MySqlPool, line_user_id: &str, event_id: i32) -> bool {
        pending_registration_repository::exists(pool, line_user_id, event_id)
            .await
            .unwrap()
    }

    async fn same_database_url(pool: &sqlx::MySqlPool) -> String {
        let row = sqlx::query("SELECT DATABASE() AS db_name")
            .fetch_one(pool)
            .await
            .unwrap();
        let db_name: String = row.try_get("db_name").unwrap();
        let database_url = std::env::var("DATABASE_URL").unwrap();
        let (base, query) = database_url
            .split_once('?')
            .map_or((database_url.as_str(), ""), |(base, query)| (base, query));
        let slash_index = base.rfind('/').unwrap();
        let query = if query.is_empty() {
            String::new()
        } else {
            format!("?{query}")
        };

        format!("{}{db_name}{query}", &base[..=slash_index])
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
            None,
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
        let player_id = crate::repository::player_repository::insert(
            pool,
            line_user_id,
            event_id,
            "Alice",
            &format!("stamp-token-{line_user_id}"),
        )
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
            ReplyMessage::StampStatus { image_url } => panic!("expected text reply: {image_url}"),
            ReplyMessage::Cleared { elapsed, .. } => panic!("expected text reply: {elapsed}"),
        }
    }

    fn quest(reply: ReplyMessage) -> (String, String, String, Option<String>) {
        match reply {
            ReplyMessage::Quest {
                intro,
                room_name,
                quest_text,
                image_url,
                ..
            } => (intro, room_name, quest_text, image_url),
            ReplyMessage::Text(value) => panic!("expected quest reply: {value}"),
            ReplyMessage::StampStatus { image_url } => panic!("expected quest reply: {image_url}"),
            ReplyMessage::Cleared { elapsed, .. } => panic!("expected quest reply: {elapsed}"),
        }
    }

    fn stamp_status(reply: ReplyMessage) -> String {
        match reply {
            ReplyMessage::StampStatus { image_url } => image_url,
            ReplyMessage::Text(value) => panic!("expected stamp status reply: {value}"),
            ReplyMessage::Quest { room_name, .. } => {
                panic!("expected stamp status reply: {room_name}")
            }
            ReplyMessage::Cleared { elapsed, .. } => {
                panic!("expected stamp status reply: {elapsed}")
            }
        }
    }

    fn is_elapsed_display(value: &str) -> bool {
        let parts: Vec<_> = value.split(':').collect();
        matches!(parts.len(), 2 | 3)
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|char| char.is_ascii_digit()))
    }

    #[sqlx::test]
    async fn starts_registration_for_individual_event(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        let reply =
            super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-start-individual", "開始")
                .await
                .unwrap();

        assert!(text(reply).contains("個人名"));
        assert!(pending_exists(&pool, "line-start-individual", event_id).await);
    }

    #[sqlx::test]
    async fn starts_registration_for_team_event(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, true, false).await;
        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-start-team", "開始")
            .await
            .unwrap();

        assert!(text(reply).contains("チーム名"));
    }

    #[sqlx::test]
    async fn pending_name_creates_player_assigns_room_and_returns_quest(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_room(&pool, event_id, "Library", None, None).await;
        insert_pending(&pool, "line-name", event_id).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-name", "Alice")
            .await
            .unwrap();
        let (intro, room_name, quest_text, image_url) = quest(reply);
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-name",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(intro, "最初の部屋は");
        assert_eq!(room_name, "Library");
        assert_eq!(quest_text, "Find the red book");
        assert!(image_url.is_none());
        assert!(player.current_room_id.is_some());
        assert!(!pending_exists(&pool, "line-name", event_id).await);
    }

    #[sqlx::test]
    async fn first_quest_reply_contains_stamp_card_url_with_player_token(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_room(&pool, event_id, "Library", None, None).await;
        insert_pending(&pool, "line-stamp-first", event_id).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-stamp-first", "Alice")
            .await
            .unwrap();
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-stamp-first",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        let ReplyMessage::Quest { stamp_card_url, .. } = reply else {
            panic!("expected quest reply");
        };
        assert_eq!(
            stamp_card_url,
            format!(
                "{PUBLIC_BASE_URL}/public/stamp-card/{}",
                player.stamp_card_token
            )
        );
    }

    #[sqlx::test]
    async fn registered_start_reuses_existing_stamp_card_token(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_player_with_room(&pool, event_id, "line-stamp-start", false).await;
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-stamp-start",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-stamp-start", "開始")
            .await
            .unwrap();

        let ReplyMessage::Quest { stamp_card_url, .. } = reply else {
            panic!("expected quest reply");
        };
        assert_eq!(
            stamp_card_url,
            format!(
                "{PUBLIC_BASE_URL}/public/stamp-card/{}",
                player.stamp_card_token
            )
        );
    }

    #[sqlx::test]
    async fn pending_blank_name_prompts_again_without_creating_player(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        insert_pending(&pool, "line-blank", event_id).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-blank", "   ")
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
        assert!(pending_exists(&pool, "line-blank", event_id).await);
    }

    #[sqlx::test]
    async fn pending_name_without_rooms_returns_error_without_creating_player(
        pool: sqlx::MySqlPool,
    ) {
        let event_id = set_event_flags(&pool, false, false).await;
        insert_pending(&pool, "line-no-room", event_id).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-no-room", "Alice")
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
        assert!(!pending_exists(&pool, "line-no-room", event_id).await);
    }

    #[sqlx::test]
    async fn registered_start_resends_current_room_quest(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        let (_player_id, room_id) =
            seed_player_with_room(&pool, event_id, "line-registered-start", false).await;

        let reply =
            super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-registered-start", "開始")
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

        let (intro, room_name, _quest_text, _image_url) = quest(reply);
        assert_eq!(intro, "現在向かっている部屋は");
        assert_eq!(room_name, "Library");
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

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-finished", "開始")
            .await
            .unwrap();

        assert!(text(reply).contains("クリア済み"));
    }

    #[sqlx::test]
    async fn registered_reset_deletes_player(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_player_with_room(&pool, event_id, "line-reset", false).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-reset", "リセット")
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

        let reply =
            super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-reset-missing", "リセット")
                .await
                .unwrap();

        assert!(text(reply).contains("参加登録されていません"));
    }

    #[sqlx::test]
    async fn pending_reset_cancels_registration(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        insert_pending(&pool, "line-reset-pending", event_id).await;

        let reply =
            super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-reset-pending", "リセット")
                .await
                .unwrap();

        assert!(text(reply).contains("キャンセル"));
        assert!(!pending_exists(&pool, "line-reset-pending", event_id).await);
    }

    #[sqlx::test]
    async fn repeated_start_keeps_single_pending_registration(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        insert_pending(&pool, "line-repeat-start", event_id).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-repeat-start", "開始")
            .await
            .unwrap();
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS count
            FROM pending_registrations
            WHERE line_user_id = ? AND event_id = ?
            "#,
        )
        .bind("line-repeat-start")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(text(reply).contains("個人名"));
        assert_eq!(row.try_get::<i64, _>("count").unwrap(), 1);
    }

    #[sqlx::test]
    async fn pending_registration_survives_pool_recreation(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_room(&pool, event_id, "Library", None, None).await;
        let database_url = same_database_url(&pool).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-restarted", "開始")
            .await
            .unwrap();
        assert!(text(reply).contains("個人名"));
        assert!(pending_exists(&pool, "line-restarted", event_id).await);

        let fresh_pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
            .unwrap();
        let reply =
            super::handle_text_message(&fresh_pool, PUBLIC_BASE_URL, "line-restarted", "Alice")
                .await
                .unwrap();
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &fresh_pool,
            "line-restarted",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        let (intro, room_name, _quest_text, _image_url) = quest(reply);
        assert_eq!(intro, "最初の部屋は");
        assert_eq!(room_name, "Library");
        assert_eq!(player.player_name, "Alice");
        assert!(!pending_exists(&fresh_pool, "line-restarted", event_id).await);
    }

    #[sqlx::test]
    async fn help_always_returns_guide(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, false, false).await;
        for command in ["遊び方", "ヘルプ"] {
            let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-help", command)
                .await
                .unwrap();
            assert!(text(reply).contains("開始"));
        }
    }

    #[sqlx::test]
    async fn registered_player_can_request_stamp_status(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_player_with_room(&pool, event_id, "line-stamp-status", false).await;
        let player = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-stamp-status",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        let reply =
            super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-stamp-status", "スタンプ状況")
                .await
                .unwrap();

        assert_eq!(
            stamp_status(reply),
            format!(
                "{PUBLIC_BASE_URL}/public/stamp-card/{}",
                player.stamp_card_token
            )
        );
    }

    #[sqlx::test]
    async fn unregistered_stamp_status_prompts_start(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, false, false).await;

        let reply = super::handle_text_message(
            &pool,
            PUBLIC_BASE_URL,
            "line-stamp-missing",
            "スタンプ状況",
        )
        .await
        .unwrap();

        assert_eq!(text(reply), "『開始』と送信して参加登録してください。");
    }

    #[sqlx::test]
    async fn finished_stamp_status_returns_finished_message(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        let (player_id, _room_id) =
            seed_player_with_room(&pool, event_id, "line-stamp-finished", false).await;
        crate::repository::player_repository::mark_finished(&pool, player_id)
            .await
            .unwrap();

        let reply = super::handle_text_message(
            &pool,
            PUBLIC_BASE_URL,
            "line-stamp-finished",
            "スタンプ状況",
        )
        .await
        .unwrap();

        assert_eq!(text(reply), "クリア済みです。最初の部屋に戻ってください。");
    }

    #[sqlx::test]
    async fn help_includes_stamp_status_command(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, false, false).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-help-stamp", "ヘルプ")
            .await
            .unwrap();

        assert!(text(reply).contains("スタンプ状況"));
    }

    #[sqlx::test]
    async fn hint_is_unavailable_without_answer_check(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_player_with_room(&pool, event_id, "line-hint-off", false).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-hint-off", "ヒント")
            .await
            .unwrap();

        assert!(text(reply).contains("利用できません"));
    }

    #[sqlx::test]
    async fn hint_returns_current_room_hint_when_answer_check_is_required(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, true).await;
        seed_player_with_room(&pool, event_id, "line-hint-on", true).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-hint-on", "ヒント")
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
            "stamp-token-line-hint-missing",
        )
        .await
        .unwrap();
        crate::repository::player_repository::update_current_room(&pool, player_id, room_id)
            .await
            .unwrap();

        let reply =
            super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-hint-missing", "ヒント")
                .await
                .unwrap();

        assert!(text(reply).contains("登録されていません"));
    }

    #[sqlx::test]
    async fn correct_answer_sets_verified(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, true).await;
        let (player_id, _room_id) =
            seed_player_with_room(&pool, event_id, "line-correct", true).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-correct", " red ")
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

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-wrong", "green")
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

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-verified", "anything")
            .await
            .unwrap();

        assert!(text(reply).contains("QR"));
    }

    #[sqlx::test]
    async fn free_text_without_answer_check_prompts_qr(pool: sqlx::MySqlPool) {
        let event_id = set_event_flags(&pool, false, false).await;
        seed_player_with_room(&pool, event_id, "line-free", false).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-free", "anything")
            .await
            .unwrap();

        assert!(text(reply).contains("QRコード"));
    }

    #[sqlx::test]
    async fn unregistered_free_text_prompts_start(pool: sqlx::MySqlPool) {
        set_event_flags(&pool, false, false).await;

        let reply = super::handle_text_message(&pool, PUBLIC_BASE_URL, "line-unknown", "hello")
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
            None,
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
            &format!("stamp-token-{line_user_id}"),
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
        let player_id =
            seed_player_current_room(&pool, event_id, "line-checkin-next", current_room).await;
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

        assert_eq!(
            crate::repository::player_repository::count_visited(&pool, player_id)
                .await
                .unwrap(),
            1
        );
        assert_eq!(player.current_room_id, Some(next_room));
        assert!(!player.answer_verified);
        let super::CheckinOutcome::NextQuest(ReplyMessage::Quest { intro, .. }) = outcome else {
            panic!("expected next quest outcome");
        };
        assert_eq!(intro, "【Library】クリアおめでとうございます。次の部屋は");
    }

    #[sqlx::test]
    async fn checkin_next_quest_reuses_existing_stamp_card_token(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let current_room = seed_named_room(&pool, event_id, "Library", "qr-stamp-current").await;
        seed_named_room(&pool, event_id, "Gym", "qr-stamp-next").await;
        seed_player_current_room(&pool, event_id, "line-stamp-checkin", current_room).await;
        let before = crate::repository::player_repository::find_by_line_user_and_event(
            &pool,
            "line-stamp-checkin",
            event_id,
        )
        .await
        .unwrap()
        .unwrap();

        let outcome = super::checkin(
            &pool,
            PUBLIC_BASE_URL,
            "line-stamp-checkin",
            "qr-stamp-current",
        )
        .await
        .unwrap();

        let super::CheckinOutcome::NextQuest(ReplyMessage::Quest { stamp_card_url, .. }) = outcome
        else {
            panic!("expected next quest outcome");
        };
        assert_eq!(
            stamp_card_url,
            format!(
                "{PUBLIC_BASE_URL}/public/stamp-card/{}",
                before.stamp_card_token
            )
        );
    }

    #[sqlx::test]
    async fn checkin_last_room_marks_finished_and_returns_cleared(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let only_room = seed_named_room(&pool, event_id, "Library", "qr-last").await;
        let player_id =
            seed_player_current_room(&pool, event_id, "line-checkin-clear", only_room).await;
        let started_at = chrono::Utc::now().naive_utc() - chrono::TimeDelta::seconds(65);
        sqlx::query("UPDATE players SET started_at = ? WHERE id = ?")
            .bind(started_at)
            .bind(player_id)
            .execute(&pool)
            .await
            .unwrap();

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

        let super::CheckinOutcome::Cleared(ReplyMessage::Cleared {
            elapsed,
            stamp_card_url,
        }) = outcome
        else {
            panic!("expected cleared outcome");
        };
        assert!(is_elapsed_display(&elapsed));
        assert_eq!(
            stamp_card_url,
            format!(
                "{PUBLIC_BASE_URL}/public/stamp-card/{}",
                player.stamp_card_token
            )
        );
        assert_eq!(
            crate::repository::player_repository::count_visited(&pool, player_id)
                .await
                .unwrap(),
            1
        );
        assert!(player.finished_at.is_some());
    }

    #[sqlx::test]
    async fn checkin_rejects_missing_room_without_db_change(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let current_room = seed_named_room(&pool, event_id, "Library", "qr-existing").await;
        let player_id =
            seed_player_current_room(&pool, event_id, "line-missing-room", current_room).await;

        let outcome = super::checkin(&pool, PUBLIC_BASE_URL, "line-missing-room", "missing-qr")
            .await
            .unwrap();

        assert_eq!(
            outcome,
            super::CheckinOutcome::Rejected(super::CheckinRejection::RoomNotFound)
        );
        assert_eq!(
            crate::repository::player_repository::count_visited(&pool, player_id)
                .await
                .unwrap(),
            0
        );
    }

    #[sqlx::test]
    async fn checkin_rejects_unregistered_player(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        seed_named_room(&pool, event_id, "Library", "qr-unregistered").await;

        let outcome = super::checkin(
            &pool,
            PUBLIC_BASE_URL,
            "line-not-registered",
            "qr-unregistered",
        )
        .await
        .unwrap();

        assert_eq!(
            outcome,
            super::CheckinOutcome::Rejected(super::CheckinRejection::NotRegistered)
        );
    }

    #[sqlx::test]
    async fn checkin_rejects_wrong_room_without_visit(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let current_room = seed_named_room(&pool, event_id, "Library", "qr-current-wrong").await;
        seed_named_room(&pool, event_id, "Gym", "qr-wrong").await;
        let player_id =
            seed_player_current_room(&pool, event_id, "line-wrong-room", current_room).await;

        let outcome = super::checkin(&pool, PUBLIC_BASE_URL, "line-wrong-room", "qr-wrong")
            .await
            .unwrap();

        assert_eq!(
            outcome,
            super::CheckinOutcome::Rejected(super::CheckinRejection::WrongRoom)
        );
        assert_eq!(
            crate::repository::player_repository::count_visited(&pool, player_id)
                .await
                .unwrap(),
            0
        );
    }

    #[sqlx::test]
    async fn checkin_rejects_when_answer_not_verified(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        set_require_answer_check(&pool, event_id, true).await;
        let room = seed_named_room(&pool, event_id, "Library", "qr-answer-required").await;
        let player_id =
            seed_player_current_room(&pool, event_id, "line-answer-not-verified", room).await;

        let outcome = super::checkin(
            &pool,
            PUBLIC_BASE_URL,
            "line-answer-not-verified",
            "qr-answer-required",
        )
        .await
        .unwrap();

        assert_eq!(
            outcome,
            super::CheckinOutcome::Rejected(super::CheckinRejection::AnswerNotVerified)
        );
        assert_eq!(
            crate::repository::player_repository::count_visited(&pool, player_id)
                .await
                .unwrap(),
            0
        );
    }

    #[sqlx::test]
    async fn checkin_allows_verified_answer_mode(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        set_require_answer_check(&pool, event_id, true).await;
        let room = seed_named_room(&pool, event_id, "Library", "qr-answer-verified").await;
        let player_id =
            seed_player_current_room(&pool, event_id, "line-answer-verified", room).await;
        crate::repository::player_repository::set_answer_verified(&pool, player_id, true)
            .await
            .unwrap();

        let outcome = super::checkin(
            &pool,
            PUBLIC_BASE_URL,
            "line-answer-verified",
            "qr-answer-verified",
        )
        .await
        .unwrap();

        assert!(matches!(outcome, super::CheckinOutcome::Cleared(_)));
        assert_eq!(
            crate::repository::player_repository::count_visited(&pool, player_id)
                .await
                .unwrap(),
            1
        );
    }

    #[sqlx::test]
    async fn checkin_rejects_already_finished_player(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room = seed_named_room(&pool, event_id, "Library", "qr-finished").await;
        let player_id =
            seed_player_current_room(&pool, event_id, "line-already-finished", room).await;
        crate::repository::player_repository::mark_finished(&pool, player_id)
            .await
            .unwrap();
        sqlx::query("UPDATE players SET current_room_id = NULL WHERE id = ?")
            .bind(player_id)
            .execute(&pool)
            .await
            .unwrap();

        let outcome = super::checkin(
            &pool,
            PUBLIC_BASE_URL,
            "line-already-finished",
            "qr-finished",
        )
        .await
        .unwrap();

        assert_eq!(
            outcome,
            super::CheckinOutcome::Rejected(super::CheckinRejection::AlreadyFinished)
        );
    }
}
