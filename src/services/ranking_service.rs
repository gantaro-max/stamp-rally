use chrono::TimeDelta;
use sqlx::MySqlPool;

use crate::repository::player_repository;

#[derive(Debug, PartialEq, Eq)]
pub struct RankedEntry {
    pub rank: usize,
    pub player_name: String,
    pub elapsed_display: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UnfinishedEntry {
    pub player_name: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RankingView {
    pub ranked: Vec<RankedEntry>,
    pub unfinished: Vec<UnfinishedEntry>,
}

#[derive(Debug)]
pub enum RankingError {
    Database(sqlx::Error),
}

impl From<sqlx::Error> for RankingError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err)
    }
}

impl std::fmt::Display for RankingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(err) => write!(f, "database error: {err}"),
        }
    }
}

impl std::error::Error for RankingError {}

pub fn build_ranking(players: Vec<player_repository::Player>) -> RankingView {
    let mut finished = Vec::new();
    let mut unfinished = Vec::new();

    for player in players {
        match player.finished_at {
            Some(finished_at) => finished.push((finished_at - player.started_at, player)),
            None => unfinished.push(player),
        }
    }

    finished.sort_by_key(|(elapsed, player)| (*elapsed, player.started_at, player.id));
    unfinished.sort_by_key(|player| (player.started_at, player.id));

    RankingView {
        ranked: finished
            .into_iter()
            .enumerate()
            .map(|(index, (elapsed, player))| RankedEntry {
                rank: index + 1,
                player_name: player.player_name,
                elapsed_display: format_elapsed(elapsed),
            })
            .collect(),
        unfinished: unfinished
            .into_iter()
            .map(|player| UnfinishedEntry {
                player_name: player.player_name,
            })
            .collect(),
    }
}

pub async fn get_ranking(pool: &MySqlPool, event_id: i32) -> Result<RankingView, RankingError> {
    let players = player_repository::find_all_by_event(pool, event_id).await?;
    Ok(build_ranking(players))
}

pub(crate) fn format_elapsed(duration: TimeDelta) -> String {
    let total_seconds = duration.num_seconds().max(0);
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDateTime;

    use crate::repository::player_repository::Player;

    fn at(value: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    fn player(id: i32, name: &str, started_at: &str, finished_at: Option<&str>) -> Player {
        Player {
            id,
            line_user_id: format!("line-{id}"),
            event_id: 1,
            player_name: name.to_string(),
            current_room_id: None,
            answer_verified: false,
            started_at: at(started_at),
            finished_at: finished_at.map(at),
            stamp_card_token: format!("stamp-token-{id}"),
        }
    }

    #[test]
    fn build_ranking_orders_finished_players_by_elapsed_time() {
        let ranking = super::build_ranking(vec![
            player(
                1,
                "Slow",
                "2026-01-01 10:00:00",
                Some("2026-01-01 10:30:00"),
            ),
            player(
                2,
                "Fast",
                "2026-01-01 10:10:00",
                Some("2026-01-01 10:20:00"),
            ),
        ]);

        assert_eq!(ranking.ranked[0].rank, 1);
        assert_eq!(ranking.ranked[0].player_name, "Fast");
        assert_eq!(ranking.ranked[0].elapsed_display, "10:00");
        assert_eq!(ranking.ranked[1].rank, 2);
        assert_eq!(ranking.ranked[1].player_name, "Slow");
        assert_eq!(ranking.ranked[1].elapsed_display, "30:00");
    }

    #[test]
    fn build_ranking_sorts_unfinished_players_by_started_at() {
        let ranking = super::build_ranking(vec![
            player(1, "Later", "2026-01-01 10:10:00", None),
            player(2, "Earlier", "2026-01-01 10:00:00", None),
            player(
                3,
                "Finished",
                "2026-01-01 10:00:00",
                Some("2026-01-01 10:05:00"),
            ),
        ]);

        assert_eq!(ranking.unfinished.len(), 2);
        assert_eq!(ranking.unfinished[0].player_name, "Earlier");
        assert_eq!(ranking.unfinished[1].player_name, "Later");
    }

    #[test]
    fn build_ranking_handles_only_unfinished_players() {
        let ranking = super::build_ranking(vec![
            player(1, "Alice", "2026-01-01 10:00:00", None),
            player(2, "Bob", "2026-01-01 10:05:00", None),
        ]);

        assert!(ranking.ranked.is_empty());
        assert_eq!(ranking.unfinished.len(), 2);
        assert_eq!(ranking.unfinished[0].player_name, "Alice");
        assert_eq!(ranking.unfinished[1].player_name, "Bob");
    }

    #[test]
    fn build_ranking_formats_elapsed_time_over_one_hour() {
        let ranking = super::build_ranking(vec![
            player(
                1,
                "Short",
                "2026-01-01 10:00:00",
                Some("2026-01-01 10:05:09"),
            ),
            player(
                2,
                "Long",
                "2026-01-01 10:00:00",
                Some("2026-01-01 11:02:03"),
            ),
        ]);

        assert_eq!(ranking.ranked[0].elapsed_display, "5:09");
        assert_eq!(ranking.ranked[1].elapsed_display, "1:02:03");
    }

    #[test]
    fn build_ranking_assigns_sequential_ranks_for_tied_elapsed_time() {
        let ranking = super::build_ranking(vec![
            player(
                1,
                "Alice",
                "2026-01-01 10:00:00",
                Some("2026-01-01 10:10:00"),
            ),
            player(2, "Bob", "2026-01-01 10:05:00", Some("2026-01-01 10:15:00")),
        ]);

        assert_eq!(ranking.ranked[0].rank, 1);
        assert_eq!(ranking.ranked[1].rank, 2);
    }

    #[sqlx::test]
    async fn get_ranking_matches_build_ranking(pool: sqlx::MySqlPool) {
        crate::services::auth_service::seed_admin_event_if_empty(
            &pool,
            "admin-secret",
            "Stamp Rally",
        )
        .await
        .unwrap();
        let event_id = crate::repository::event_repository::find_singleton(&pool)
            .await
            .unwrap()
            .unwrap()
            .id;
        let finished_id = crate::repository::player_repository::insert(
            &pool,
            "line-finished-ranking",
            event_id,
            "Alice",
            "stamp-token-line-finished-ranking",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE players SET started_at = ?, finished_at = ? WHERE id = ?")
            .bind(at("2026-01-01 10:00:00"))
            .bind(at("2026-01-01 10:07:00"))
            .bind(finished_id)
            .execute(&pool)
            .await
            .unwrap();
        let unfinished_id = crate::repository::player_repository::insert(
            &pool,
            "line-unfinished-ranking",
            event_id,
            "Bob",
            "stamp-token-line-unfinished-ranking",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE players SET started_at = ? WHERE id = ?")
            .bind(at("2026-01-01 10:02:00"))
            .bind(unfinished_id)
            .execute(&pool)
            .await
            .unwrap();

        let players = crate::repository::player_repository::find_all_by_event(&pool, event_id)
            .await
            .unwrap();
        let expected = super::build_ranking(players);
        let actual = super::get_ranking(&pool, event_id).await.unwrap();

        assert_eq!(actual, expected);
    }
}
