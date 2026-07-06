use sqlx::{MySqlPool, Row};

#[allow(dead_code)]
#[derive(Debug, sqlx::FromRow)]
pub struct Room {
    pub id: i32,
    pub event_id: i32,
    pub room_name: String,
    pub quest_text: String,
    pub answer: Option<String>,
    pub hint_msg: Option<String>,
    pub image_id: Option<i32>,
    pub qr_uuid: String,
}

pub async fn count(pool: &MySqlPool, event_id: i32) -> Result<i64, sqlx::Error> {
    let row = sqlx::query("SELECT COUNT(*) AS count FROM rooms WHERE event_id = ?")
        .bind(event_id)
        .fetch_one(pool)
        .await?;

    row.try_get("count")
}

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    pool: &MySqlPool,
    event_id: i32,
    room_name: &str,
    quest_text: &str,
    answer: Option<&str>,
    hint_msg: Option<&str>,
    image_id: Option<i32>,
    qr_uuid: &str,
) -> Result<i32, sqlx::Error> {
    let result = sqlx::query(
        r#"
        INSERT INTO rooms (event_id, room_name, quest_text, answer, hint_msg, image_id, qr_uuid)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(event_id)
    .bind(room_name)
    .bind(quest_text)
    .bind(answer)
    .bind(hint_msg)
    .bind(image_id)
    .bind(qr_uuid)
    .execute(pool)
    .await?;

    Ok(result.last_insert_id() as i32)
}

pub async fn update(
    pool: &MySqlPool,
    id: i32,
    room_name: &str,
    quest_text: &str,
    answer: Option<&str>,
    hint_msg: Option<&str>,
    image_id: Option<i32>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE rooms
        SET room_name = ?, quest_text = ?, answer = ?, hint_msg = ?, image_id = ?
        WHERE id = ?
        "#,
    )
    .bind(room_name)
    .bind(quest_text)
    .bind(answer)
    .bind(hint_msg)
    .bind(image_id)
    .bind(id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete(pool: &MySqlPool, id: i32) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM rooms WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn find_all(pool: &MySqlPool, event_id: i32) -> Result<Vec<Room>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id, event_id, room_name, quest_text, answer, hint_msg, image_id, qr_uuid
        FROM rooms
        WHERE event_id = ?
        ORDER BY id
        "#,
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(Room {
                id: row.try_get("id")?,
                event_id: row.try_get("event_id")?,
                room_name: row.try_get("room_name")?,
                quest_text: row.try_get("quest_text")?,
                answer: row.try_get("answer")?,
                hint_msg: row.try_get("hint_msg")?,
                image_id: row.try_get("image_id")?,
                qr_uuid: row.try_get("qr_uuid")?,
            })
        })
        .collect()
}

pub async fn find_by_id(pool: &MySqlPool, id: i32) -> Result<Option<Room>, sqlx::Error> {
    let Some(row) = sqlx::query(
        r#"
        SELECT id, event_id, room_name, quest_text, answer, hint_msg, image_id, qr_uuid
        FROM rooms
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(Room {
        id: row.try_get("id")?,
        event_id: row.try_get("event_id")?,
        room_name: row.try_get("room_name")?,
        quest_text: row.try_get("quest_text")?,
        answer: row.try_get("answer")?,
        hint_msg: row.try_get("hint_msg")?,
        image_id: row.try_get("image_id")?,
        qr_uuid: row.try_get("qr_uuid")?,
    }))
}

pub async fn find_random_unvisited(
    pool: &MySqlPool,
    event_id: i32,
    player_id: i32,
) -> Result<Option<Room>, sqlx::Error> {
    let Some(row) = sqlx::query(
        r#"
        SELECT id, event_id, room_name, quest_text, answer, hint_msg, image_id, qr_uuid
        FROM rooms
        WHERE event_id = ?
          AND NOT EXISTS (
              SELECT 1
              FROM visited_rooms
              WHERE visited_rooms.player_id = ?
                AND visited_rooms.room_id = rooms.id
          )
        ORDER BY RAND()
        LIMIT 1
        "#,
    )
    .bind(event_id)
    .bind(player_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    Ok(Some(Room {
        id: row.try_get("id")?,
        event_id: row.try_get("event_id")?,
        room_name: row.try_get("room_name")?,
        quest_text: row.try_get("quest_text")?,
        answer: row.try_get("answer")?,
        hint_msg: row.try_get("hint_msg")?,
        image_id: row.try_get("image_id")?,
        qr_uuid: row.try_get("qr_uuid")?,
    }))
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

    #[sqlx::test]
    async fn inserts_and_finds_room_by_id(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;

        let room_id = super::insert(
            &pool,
            event_id,
            "Room A",
            "Find the red mark",
            Some("red"),
            Some("look up"),
            None,
            "qr-uuid-1",
        )
        .await
        .unwrap();

        let room = super::find_by_id(&pool, room_id).await.unwrap().unwrap();

        assert_eq!(room.id, room_id);
        assert_eq!(room.event_id, event_id);
        assert_eq!(room.room_name, "Room A");
        assert_eq!(room.quest_text, "Find the red mark");
        assert_eq!(room.answer.as_deref(), Some("red"));
        assert_eq!(room.hint_msg.as_deref(), Some("look up"));
        assert_eq!(room.image_id, None);
        assert_eq!(room.qr_uuid, "qr-uuid-1");
    }

    #[sqlx::test]
    async fn counts_rooms_for_event(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;

        for index in 0..2 {
            super::insert(
                &pool,
                event_id,
                &format!("Room {index}"),
                "Quest",
                None,
                None,
                None,
                &format!("qr-count-{index}"),
            )
            .await
            .unwrap();
        }

        assert_eq!(super::count(&pool, event_id).await.unwrap(), 2);
    }

    #[sqlx::test]
    async fn updates_room_fields(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room_id = super::insert(
            &pool,
            event_id,
            "Old Room",
            "Old Quest",
            Some("old"),
            Some("old hint"),
            None,
            "qr-update-1",
        )
        .await
        .unwrap();

        super::update(
            &pool,
            room_id,
            "New Room",
            "New Quest",
            Some("new"),
            Some("new hint"),
            None,
        )
        .await
        .unwrap();

        let room = super::find_by_id(&pool, room_id).await.unwrap().unwrap();
        assert_eq!(room.room_name, "New Room");
        assert_eq!(room.quest_text, "New Quest");
        assert_eq!(room.answer.as_deref(), Some("new"));
        assert_eq!(room.hint_msg.as_deref(), Some("new hint"));
        assert_eq!(room.image_id, None);
    }

    #[sqlx::test]
    async fn deletes_room(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let room_id = super::insert(
            &pool,
            event_id,
            "Room to delete",
            "Quest",
            None,
            None,
            None,
            "qr-delete-1",
        )
        .await
        .unwrap();

        super::delete(&pool, room_id).await.unwrap();

        assert!(super::find_by_id(&pool, room_id).await.unwrap().is_none());
    }

    async fn seed_player(pool: &sqlx::MySqlPool, event_id: i32) -> i32 {
        let result = sqlx::query(
            "INSERT INTO players (line_user_id, event_id, player_name, started_at) VALUES (?, ?, ?, NOW())",
        )
        .bind("line-user-room-random")
        .bind(event_id)
        .bind("Player")
        .execute(pool)
        .await
        .unwrap();
        result.last_insert_id() as i32
    }

    #[sqlx::test]
    async fn finds_random_unvisited_room(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let player_id = seed_player(&pool, event_id).await;
        let visited_room_id = super::insert(&pool, event_id, "Visited", "Quest", None, None, None, "qr-random-1").await.unwrap();
        let unvisited_a = super::insert(&pool, event_id, "Unvisited A", "Quest", None, None, None, "qr-random-2").await.unwrap();
        let unvisited_b = super::insert(&pool, event_id, "Unvisited B", "Quest", None, None, None, "qr-random-3").await.unwrap();
        sqlx::query("INSERT INTO visited_rooms (player_id, room_id, visited_at) VALUES (?, ?, NOW())")
            .bind(player_id)
            .bind(visited_room_id)
            .execute(&pool)
            .await
            .unwrap();

        let room = super::find_random_unvisited(&pool, event_id, player_id).await.unwrap().unwrap();

        assert!([unvisited_a, unvisited_b].contains(&room.id));
    }

    #[sqlx::test]
    async fn find_random_unvisited_returns_none_when_all_rooms_visited(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let player_id = seed_player(&pool, event_id).await;
        for index in 0..3 {
            let room_id = super::insert(&pool, event_id, &format!("Room {index}"), "Quest", None, None, None, &format!("qr-all-visited-{index}")).await.unwrap();
            sqlx::query("INSERT INTO visited_rooms (player_id, room_id, visited_at) VALUES (?, ?, NOW())")
                .bind(player_id)
                .bind(room_id)
                .execute(&pool)
                .await
                .unwrap();
        }

        let room = super::find_random_unvisited(&pool, event_id, player_id).await.unwrap();

        assert!(room.is_none());
    }

    #[sqlx::test]
    async fn find_random_unvisited_returns_none_when_event_has_no_rooms(pool: sqlx::MySqlPool) {
        let event_id = seed_event(&pool).await;
        let player_id = seed_player(&pool, event_id).await;

        let room = super::find_random_unvisited(&pool, event_id, player_id).await.unwrap();

        assert!(room.is_none());
    }

}