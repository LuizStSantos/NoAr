use sqlx::{
    SqliteConnection,
    sqlite::{SqlitePool, SqliteQueryResult},
};

use crate::{
    db::models::Scte35,
    utils::{config::Scte35 as Scte35Config, errors::ProcessError},
};

pub async fn select_scte35(pool: &SqlitePool, channel_id: i32) -> Result<Scte35, ProcessError> {
    Ok(sqlx::query_as(
        "SELECT scte35.* FROM config_scte35 scte35
            JOIN config ON config.id = scte35.config_id
            WHERE config.channel_id = $1",
    )
    .bind(channel_id)
    .fetch_one(pool)
    .await?)
}

pub async fn insert_scte35(
    connection: &mut SqliteConnection,
    config_id: i32,
) -> Result<SqliteQueryResult, ProcessError> {
    Ok(
        sqlx::query("INSERT INTO config_scte35 (config_id) VALUES ($1)")
            .bind(config_id)
            .execute(connection)
            .await?,
    )
}

pub async fn update_scte35_on(
    connection: &mut SqliteConnection,
    channel_id: i32,
    scte35: &Scte35Config,
) -> Result<SqliteQueryResult, ProcessError> {
    Ok(sqlx::query("UPDATE config_scte35 SET enabled = $2, pid = $3, preroll = $4, min_break = $5, max_break = $6, api_key = $7 WHERE config_id = (SELECT id FROM config WHERE channel_id = $1)")
        .bind(channel_id).bind(scte35.enable).bind(i64::from(scte35.pid)).bind(scte35.preroll)
        .bind(scte35.min_break).bind(scte35.max_break).bind(scte35.key.trim())
        .execute(connection).await?)
}

/// Which remote-control key: the logo's (also the L-shape's) or the SCTE-35 one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteKey {
    Logo,
    Scte35,
}

/// Store `key` as the channel's `kind` key, alone: saving the rest of the
/// configuration never changes a key.
pub async fn update_remote_key(
    pool: &sqlx::SqlitePool,
    channel_id: i32,
    kind: RemoteKey,
    key: &str,
) -> Result<SqliteQueryResult, ProcessError> {
    let query = match kind {
        RemoteKey::Logo => {
            "UPDATE config_processing SET logo_key = $2 WHERE config_id = (SELECT id FROM config WHERE channel_id = $1)"
        }
        RemoteKey::Scte35 => {
            "UPDATE config_scte35 SET api_key = $2 WHERE config_id = (SELECT id FROM config WHERE channel_id = $1)"
        }
    };

    Ok(sqlx::query(query)
        .bind(channel_id)
        .bind(key)
        .execute(pool)
        .await?)
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::db::handles::db_migrate;

    #[tokio::test]
    async fn scte35_settings_are_saved_per_channel() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db_migrate(&pool).await.unwrap();

        let defaults = select_scte35(&pool, 1).await.unwrap();
        assert!(!defaults.enabled);
        assert_eq!(defaults.pid, 500);

        let mut connection = pool.acquire().await.unwrap();
        update_scte35_on(
            &mut connection,
            1,
            &Scte35Config {
                enable: true,
                pid: 600,
                preroll: 4.0,
                min_break: 15.0,
                max_break: 240.0,
                key: " k3y ".to_string(),
            },
        )
        .await
        .unwrap();
        drop(connection);

        let saved = select_scte35(&pool, 1).await.unwrap();
        assert!(saved.enabled);
        assert_eq!((saved.pid, saved.preroll), (600, 4.0));
        assert_eq!((saved.min_break, saved.max_break), (15.0, 240.0));
        assert_eq!(saved.api_key, "k3y", "saved trimmed");

        update_remote_key(&pool, 1, RemoteKey::Scte35, "n3w")
            .await
            .unwrap();
        update_remote_key(&pool, 1, RemoteKey::Logo, "l0go")
            .await
            .unwrap();
        assert_eq!(select_scte35(&pool, 1).await.unwrap().api_key, "n3w");
        let logo: String = sqlx::query_scalar(
            "SELECT logo_key FROM config_processing WHERE config_id = (SELECT id FROM config WHERE channel_id = 1)",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(logo, "l0go");
    }
}
