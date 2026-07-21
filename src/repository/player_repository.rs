use chrono::NaiveDateTime;
use sqlx::{MySqlPool, Row};

#[allow(dead_code)]
#[derive(Debug)]
pub struct Player {
    pub id: i32,
    pub line_user_id: String,
    pub event_id: i32,
    pub player_name: String,
    pub current_room_id: Option<i32>,
    pub answer_verified: bool,
    pub started_at: NaiveDateTime,
    pub finished_at: Option<NaiveDateTime>,
    pub stamp_card_token: String,
}

fn player_from_row(row: sqlx::mysql::MySqlRow) -> Result<Player, sqlx::Error> {
    Ok(Player {
        id: row.try_get("id")?,
        line_user_id: row.try_get("line_user_id")?,
        event_id: row.try_get("event_id")?,
        player_name: row.try_get("player_name")?,
        current_room_id: row.try_get("current_room_id")?,
        answer_verified: row.try_get::<i8, _>("answer_verified")? != 0,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
        stamp_card_token: row.try_get("stamp_card_token")?,
    })
}

pub async fn find_by_line_user_and_event(
    pool: &MySqlPool,
    line_user_id: &str,
    event_id: i32,
) -> Result<Option<Player>, sqlx::Error> {
    let Some(row) = sqlx::query(
        r#"
        SELECT id, line_user_id, event_id, player_name, current_room_id, answer_verified, started_at, finished_at, stamp_card_token
        FROM players
        WHERE line_user_id = ? AND event_id = ?
        "#,
    )
    .bind(line_user_id)
    .bind(event_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(player_from_row(row)?))
}

pub async fn find_all_by_event(
    pool: &MySqlPool,
    event_id: i32,
) -> Result<Vec<Player>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, line_user_id, event_id, player_name, current_room_id, answer_verified, started_at, finished_at, stamp_card_token
        FROM players
        WHERE event_id = ?
        "#,
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(player_from_row).collect()
}

pub async fn insert(
    pool: &MySqlPool,
    line_user_id: &str,
    event_id: i32,
    player_name: &str,
    stamp_card_token: &str,
) -> Result<i32, sqlx::Error> {
    let result = sqlx::query(
        r#"
        INSERT INTO players (line_user_id, event_id, player_name, current_room_id, answer_verified, started_at, finished_at, stamp_card_token)
        VALUES (?, ?, ?, NULL, FALSE, NOW(), NULL, ?)
        "#,
    )
    .bind(line_user_id)
    .bind(event_id)
    .bind(player_name)
    .bind(stamp_card_token)
    .execute(pool)
    .await?;

    Ok(result.last_insert_id() as i32)
}

pub async fn find_by_stamp_card_token(
    pool: &MySqlPool,
    stamp_card_token: &str,
) -> Result<Option<Player>, sqlx::Error> {
    let Some(row) = sqlx::query(
        r#"
        SELECT id, line_user_id, event_id, player_name, current_room_id, answer_verified, started_at, finished_at, stamp_card_token
        FROM players
        WHERE stamp_card_token = ?
        "#,
    )
    .bind(stamp_card_token)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(player_from_row(row)?))
}

pub async fn update_current_room(
    pool: &MySqlPool,
    player_id: i32,
    room_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE players SET current_room_id = ?, answer_verified = FALSE WHERE id = ?")
        .bind(room_id)
        .bind(player_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn set_answer_verified(
    pool: &MySqlPool,
    player_id: i32,
    verified: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE players SET answer_verified = ? WHERE id = ?")
        .bind(verified)
        .bind(player_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn delete_by_line_user_and_event(
    pool: &MySqlPool,
    line_user_id: &str,
    event_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM players WHERE line_user_id = ? AND event_id = ?")
        .bind(line_user_id)
        .bind(event_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn insert_visited_room(
    pool: &MySqlPool,
    player_id: i32,
    room_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO visited_rooms (player_id, room_id, visited_at) VALUES (?, ?, NOW())")
        .bind(player_id)
        .bind(room_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn count_visited(pool: &MySqlPool, player_id: i32) -> Result<i64, sqlx::Error> {
    let row = sqlx::query("SELECT COUNT(*) AS count FROM visited_rooms WHERE player_id = ?")
        .bind(player_id)
        .fetch_one(pool)
        .await?;

    row.try_get("count")
}

pub async fn mark_finished(pool: &MySqlPool, player_id: i32) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE players SET finished_at = NOW() WHERE id = ?")
        .bind(player_id)
        .execute(pool)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
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

    async fn seed_room(pool: &sqlx::MySqlPool, event_id: i32) -> i32 {
        crate::repository::room_repository::insert(
            pool,
            event_id,
            "Room",
            "Quest",
            None,
            None,
            None,
            "qr-player-room",
        )
        .await
        .unwrap()
    }

    #[sqlx::test]
    async fn inserts_and_finds_player_by_line_user_and_event(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;

        let player_id = super::insert(&pool, "line-user-1", event_id, "Alice", "token-line-user-1")
            .await
            .unwrap();
        let player = super::find_by_line_user_and_event(&pool, "line-user-1", event_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(player.id, player_id);
        assert_eq!(player.line_user_id, "line-user-1");
        assert_eq!(player.event_id, event_id);
        assert_eq!(player.player_name, "Alice");
        assert_eq!(player.current_room_id, None);
        assert!(!player.answer_verified);
        assert!(player.finished_at.is_none());
    }

    #[sqlx::test]
    async fn inserted_player_keeps_stamp_card_token(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;

        super::insert(&pool, "line-stamp-token", event_id, "Alice", "token-abc")
            .await
            .unwrap();
        let player = super::find_by_line_user_and_event(&pool, "line-stamp-token", event_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(player.stamp_card_token, "token-abc");
    }

    #[sqlx::test]
    async fn find_by_stamp_card_token_returns_matching_player(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let player_id = super::insert(&pool, "line-stamp-find", event_id, "Alice", "token-abc")
            .await
            .unwrap();

        let player = super::find_by_stamp_card_token(&pool, "token-abc")
            .await
            .unwrap()
            .unwrap();
        let missing = super::find_by_stamp_card_token(&pool, "missing-token")
            .await
            .unwrap();

        assert_eq!(player.id, player_id);
        assert_eq!(player.line_user_id, "line-stamp-find");
        assert!(missing.is_none());
    }

    #[sqlx::test]
    async fn update_current_room_sets_room_and_resets_answer_verified(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room_id = seed_room(&pool, event_id).await;
        let player_id = super::insert(&pool, "line-user-2", event_id, "Alice", "token-line-user-2")
            .await
            .unwrap();
        super::set_answer_verified(&pool, player_id, true)
            .await
            .unwrap();

        super::update_current_room(&pool, player_id, room_id)
            .await
            .unwrap();
        let player = super::find_by_line_user_and_event(&pool, "line-user-2", event_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(player.current_room_id, Some(room_id));
        assert!(!player.answer_verified);
    }

    #[sqlx::test]
    async fn set_answer_verified_updates_flag(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let player_id = super::insert(&pool, "line-user-3", event_id, "Alice", "token-line-user-3")
            .await
            .unwrap();

        super::set_answer_verified(&pool, player_id, true)
            .await
            .unwrap();
        let player = super::find_by_line_user_and_event(&pool, "line-user-3", event_id)
            .await
            .unwrap()
            .unwrap();

        assert!(player.answer_verified);
    }

    #[sqlx::test]
    async fn delete_by_line_user_and_event_deletes_player(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        super::insert(&pool, "line-user-4", event_id, "Alice", "token-line-user-4")
            .await
            .unwrap();

        super::delete_by_line_user_and_event(&pool, "line-user-4", event_id)
            .await
            .unwrap();

        assert!(
            super::find_by_line_user_and_event(&pool, "line-user-4", event_id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test]
    async fn delete_by_line_user_and_event_cascades_visited_rooms(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room_id = seed_room(&pool, event_id).await;
        let player_id = super::insert(&pool, "line-user-5", event_id, "Alice", "token-line-user-5")
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO visited_rooms (player_id, room_id, visited_at) VALUES (?, ?, NOW())",
        )
        .bind(player_id)
        .bind(room_id)
        .execute(&pool)
        .await
        .unwrap();

        super::delete_by_line_user_and_event(&pool, "line-user-5", event_id)
            .await
            .unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM visited_rooms WHERE player_id = ?")
                .bind(player_id)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(count, 0);
    }

    #[sqlx::test]
    async fn insert_visited_room_increments_count(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room_a = seed_room(&pool, event_id).await;
        let room_b = crate::repository::room_repository::insert(
            &pool,
            event_id,
            "Room 2",
            "Quest",
            None,
            None,
            None,
            "qr-player-room-2",
        )
        .await
        .unwrap();
        let player_id = super::insert(
            &pool,
            "line-visited",
            event_id,
            "Alice",
            "token-line-visited",
        )
        .await
        .unwrap();

        assert_eq!(super::count_visited(&pool, player_id).await.unwrap(), 0);
        super::insert_visited_room(&pool, player_id, room_a)
            .await
            .unwrap();
        super::insert_visited_room(&pool, player_id, room_b)
            .await
            .unwrap();

        assert_eq!(super::count_visited(&pool, player_id).await.unwrap(), 2);
    }

    #[sqlx::test]
    async fn mark_finished_sets_finished_at(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let player_id = super::insert(
            &pool,
            "line-finished-repo",
            event_id,
            "Alice",
            "token-line-finished-repo",
        )
        .await
        .unwrap();

        super::mark_finished(&pool, player_id).await.unwrap();
        let player = super::find_by_line_user_and_event(&pool, "line-finished-repo", event_id)
            .await
            .unwrap()
            .unwrap();

        assert!(player.finished_at.is_some());
    }

    #[sqlx::test]
    async fn find_all_by_event_returns_players_for_event(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        sqlx::query(
            "INSERT INTO events (event_name, admin_pass_hash, is_team_mode, require_answer_check) VALUES (?, ?, FALSE, FALSE)",
        )
        .bind("Other Event")
        .bind("hash")
        .execute(&pool)
        .await
        .unwrap();
        let other_event_id: i32 = sqlx::query_scalar("SELECT id FROM events WHERE event_name = ?")
            .bind("Other Event")
            .fetch_one(&pool)
            .await
            .unwrap();
        super::insert(
            &pool,
            "line-ranking-1",
            event_id,
            "Alice",
            "token-line-ranking-1",
        )
        .await
        .unwrap();
        super::insert(
            &pool,
            "line-ranking-2",
            event_id,
            "Bob",
            "token-line-ranking-2",
        )
        .await
        .unwrap();
        super::insert(
            &pool,
            "line-ranking-other",
            other_event_id,
            "Carol",
            "token-line-ranking-other",
        )
        .await
        .unwrap();

        let players = super::find_all_by_event(&pool, event_id).await.unwrap();
        let names: Vec<_> = players
            .iter()
            .map(|player| player.player_name.as_str())
            .collect();

        assert_eq!(players.len(), 2);
        assert!(names.contains(&"Alice"));
        assert!(names.contains(&"Bob"));
        assert!(!names.contains(&"Carol"));
    }
}
