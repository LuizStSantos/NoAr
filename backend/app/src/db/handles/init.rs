use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use log::*;
use rand::{RngExt, distr::Alphanumeric};
use sqlx::{migrate::Migrator, sqlite::SqlitePool};

use crate::{
    db::handles::select_global,
    utils::{errors::ProcessError, is_running_in_container},
};

static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// Copies kept of the database from before an update changed it.
const KEPT_BACKUPS: usize = 3;

pub async fn db_migrate(pool: &SqlitePool) -> Result<bool, ProcessError> {
    let existing_schema: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name IN ('global', 'config_global'))",
    )
    .fetch_one(pool)
    .await?;

    if existing_schema {
        // A failed copy must not keep the channels off air after an update:
        // it is reported and the update goes on.
        match backup_before_migrations(pool).await {
            Ok(Some(path)) => info!("Database copied to {} before updating it", path.display()),
            Ok(None) => {}
            Err(error) => error!("Could not copy the database before updating it: {error}"),
        }
    }

    MIGRATOR.run(pool).await?;

    if !existing_schema {
        // Earlier migrations seed the initial channel. It is a new configuration,
        // not an existing installation whose legacy selection must be preserved.
        sqlx::query("UPDATE config_audio SET loudness_scope = 'all', loudness_enable = 1")
            .execute(pool)
            .await?;
    }
    let mut init = false;

    if select_global(pool).await.is_err() {
        let secret: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(80)
            .map(char::from)
            .collect();
        let shared = is_running_in_container();

        // The trigger and the first row go in together: a start interrupted
        // halfway must not leave a trigger behind that makes the next start
        // fail, and a trigger left by an older version is reused.
        const QUERY: &str = "CREATE TRIGGER IF NOT EXISTS config_global_row_count
        BEFORE INSERT ON config_global
        WHEN (SELECT COUNT(*) FROM config_global) >= 1
        BEGIN
            SELECT RAISE(FAIL, 'Database is already initialized!');
        END;
        INSERT INTO config_global(secret, shared, setup_completed, smtp_user)
            VALUES($1, $2, 0, 'noar@example.org');";

        let mut transaction = pool.begin().await?;

        sqlx::query(QUERY)
            .bind(secret)
            .bind(shared)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;

        init = true;
    }

    Ok(init)
}

/// Copy an existing database next to its file before migrations change it,
/// so an update can be rolled back by putting the copy back. Returns the
/// copy, or `None` when nothing is pending or the database is not a file.
async fn backup_before_migrations(pool: &SqlitePool) -> Result<Option<PathBuf>, ProcessError> {
    let file: String =
        sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
            .fetch_one(pool)
            .await?;

    if file.is_empty() {
        return Ok(None);
    }

    let tracked: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
    )
    .fetch_one(pool)
    .await?;
    let applied: HashSet<i64> = if tracked {
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = 1")
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect()
    } else {
        HashSet::new()
    };
    let pending = MIGRATOR.iter().any(|migration| {
        !migration.migration_type.is_down_migration() && !applied.contains(&migration.version)
    });

    if !pending {
        return Ok(None);
    }

    let database = Path::new(&file);
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let latest = applied.iter().max().copied().unwrap_or_default();
    let backup = PathBuf::from(format!("{file}.pre-update-{stamp}-v{latest}.bak"));

    sqlx::query("VACUUM INTO $1")
        .bind(backup.to_string_lossy().to_string())
        .execute(pool)
        .await?;
    prune_backups(database, KEPT_BACKUPS);

    Ok(Some(backup))
}

/// Remove all but the `keep` newest pre-update copies of `database`.
fn prune_backups(database: &Path, keep: usize) {
    let (Some(dir), Some(name)) = (database.parent(), database.file_name()) else {
        return;
    };
    let prefix = format!("{}.pre-update-", name.to_string_lossy());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut backups: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy())
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".bak"))
        })
        .collect();

    // The time stamp in the name sorts them oldest first.
    backups.sort();

    for old in backups.iter().take(backups.len().saturating_sub(keep)) {
        if let Err(error) = std::fs::remove_file(old) {
            warn!(
                "Could not remove the old database copy {}: {error}",
                old.display()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    #[tokio::test]
    async fn a_first_start_cut_short_after_the_trigger_still_initializes() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        // What an older version left when it stopped between the two steps.
        sqlx::query(
            "CREATE TRIGGER config_global_row_count BEFORE INSERT ON config_global
             WHEN (SELECT COUNT(*) FROM config_global) >= 1
             BEGIN SELECT RAISE(FAIL, 'Database is already initialized!'); END;",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert!(db_migrate(&pool).await.unwrap());
        assert!(select_global(&pool).await.is_ok());
        // Started again, it keeps the one configuration.
        assert!(!db_migrate(&pool).await.unwrap());
    }

    #[tokio::test]
    async fn fresh_installation_normalizes_all_sources_and_keeps_user_selection_on_restart() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db_migrate(&pool).await.unwrap();
        let scope: String =
            sqlx::query_scalar("SELECT loudness_scope FROM config_audio WHERE config_id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(scope, "all");
        sqlx::query("UPDATE config_audio SET loudness_scope = 'off' WHERE config_id = 1")
            .execute(&pool)
            .await
            .unwrap();
        db_migrate(&pool).await.unwrap();
        let scope: String =
            sqlx::query_scalar("SELECT loudness_scope FROM config_audio WHERE config_id = 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(scope, "off");
    }

    async fn file_pool(path: &Path) -> SqlitePool {
        use std::str::FromStr;

        use sqlx::sqlite::SqliteConnectOptions;

        let options = SqliteConnectOptions::from_str(&path.to_string_lossy())
            .unwrap()
            .create_if_missing(true);

        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn an_update_keeps_a_copy_of_the_database_from_before() {
        let dir = std::env::temp_dir().join(format!("noar-backup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("noar.db");
        let pool = file_pool(&path).await;

        // A database from before the newest migration, as the previous
        // release left it.
        let newest = MIGRATOR
            .iter()
            .map(|migration| migration.version)
            .max()
            .unwrap();
        let previous = Migrator::with_migrations(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version != newest)
                .cloned()
                .collect(),
        );
        previous.run(&pool).await.unwrap();
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);

        // The update brings the newest migration.
        db_migrate(&pool).await.unwrap();

        let copies: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().ends_with(".bak"))
            .collect();
        assert_eq!(copies.len(), 1);

        // The copy is the database as it was: without the newest migration.
        let copy = file_pool(&copies[0]).await;
        let applied = |pool: SqlitePool| async move {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version = $1)",
            )
            .bind(newest)
            .fetch_one(&pool)
            .await
            .unwrap()
        };
        assert!(!applied(copy.clone()).await);
        assert!(applied(pool.clone()).await);

        // Nothing pending: no new copy.
        db_migrate(&pool).await.unwrap();
        assert_eq!(
            std::fs::read_dir(&dir)
                .unwrap()
                .filter(|entry| entry
                    .as_ref()
                    .unwrap()
                    .path()
                    .to_string_lossy()
                    .ends_with(".bak"))
                .count(),
            1
        );

        copy.close().await;
        pool.close().await;
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_the_newest_copies_are_kept() {
        let dir = std::env::temp_dir().join(format!("noar-prune-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let database = dir.join("noar.db");

        for stamp in [
            "20260101-000000",
            "20260201-000000",
            "20260301-000000",
            "20260401-000000",
        ] {
            std::fs::write(dir.join(format!("noar.db.pre-update-{stamp}-v15.bak")), "").unwrap();
        }
        std::fs::write(dir.join("other.bak"), "").unwrap();

        prune_backups(&database, 3);

        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            vec![
                "noar.db.pre-update-20260201-000000-v15.bak",
                "noar.db.pre-update-20260301-000000-v15.bak",
                "noar.db.pre-update-20260401-000000-v15.bak",
                "other.bak",
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
