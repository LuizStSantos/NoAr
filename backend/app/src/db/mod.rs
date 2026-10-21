use std::{
    borrow::Cow,
    fs, io,
    path::Path,
    str::FromStr,
    sync::{LazyLock, OnceLock},
    time::Duration,
};

use faccess::PathExt;
use inquire::Confirm;
use log::*;
use sqlx::{
    Pool, Sqlite,
    migrate::MigrateDatabase,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

pub mod handles;
pub mod models;

use crate::{ARGS, utils::errors::ProcessError};
use models::GlobalSettings;

const SQLITE_MAX_CONNECTIONS: u32 = 3;
const SQLITE_MIN_CONNECTIONS: u32 = 1;
const SQLITE_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const SQLITE_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

pub static DB_PATH: LazyLock<Result<Cow<'static, Path>, io::Error>> = LazyLock::new(|| {
    const DEFAULT_DIR: &str = "/usr/share/noar/db/";
    const ASSET_DIR: &str = "./assets";

    let path = if let Some(path) = ARGS.db.as_deref() {
        let mut path = Cow::Borrowed(path);
        if !path.is_absolute() {
            path = std::env::current_dir()?.join(path).into();
        }
        if path.is_dir() {
            path = database_in(&path).into();
        }
        path
    } else {
        let sys_path = Path::new(DEFAULT_DIR);
        let legacy_path = Path::new(LEGACY_DIR);
        let asset_path = Path::new(ASSET_DIR);

        if legacy_path.join(LEGACY_DB_NAME).is_file() && !sys_path.join(DB_NAME).is_file() {
            database_in(legacy_path).into()
        } else if sys_path.is_dir() && sys_path.writable() {
            database_in(sys_path).into()
        } else if asset_path.is_dir() {
            database_in(asset_path).into()
        } else {
            database_in(Path::new("")).into()
        }
    };

    if path.is_file() {
        path.access(faccess::AccessMode::WRITE)?;
    } else {
        ensure_database_parent(&path)?;
    }

    info!("Database path: {}", path.display());

    Ok(path)
});

const DB_NAME: &str = "noar.db";
/// Where installs from before the rename keep their database, still opened
/// when it is there and no `noar.db` is.
const LEGACY_DIR: &str = "/usr/share/ffplayout/db/";
const LEGACY_DB_NAME: &str = "ffplayout.db";

/// The database in `dir`: `noar.db`, or the `ffplayout.db` of an install from
/// before the rename when only that one exists.
fn database_in(dir: &Path) -> std::path::PathBuf {
    let current = dir.join(DB_NAME);
    let legacy = dir.join(LEGACY_DB_NAME);

    if !current.is_file() && legacy.is_file() {
        legacy
    } else {
        current
    }
}

fn ensure_database_parent(path: &Path) -> io::Result<()> {
    // `Path::parent()` is an empty path for a plain file name such as
    // `noar.db`. That means the current working directory is the parent,
    // not that the database has no valid location. Create explicitly supplied
    // parent directories on first run as well.
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    parent.access(faccess::AccessMode::WRITE)
}

pub static GLOBAL_SETTINGS: OnceLock<GlobalSettings> = OnceLock::new();
pub async fn db_pool() -> Result<Pool<Sqlite>, ProcessError> {
    let db_path = DB_PATH
        .as_ref()
        .map_err(|e| ProcessError::IO(e.to_string()))?;
    let db_path = db_path.to_string_lossy();

    if !Sqlite::database_exists(&db_path).await? {
        Sqlite::create_database(&db_path).await?;
    }

    let options = SqliteConnectOptions::from_str(&db_path)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(SQLITE_BUSY_TIMEOUT);

    let conn = SqlitePoolOptions::new()
        .max_connections(SQLITE_MAX_CONNECTIONS)
        .min_connections(SQLITE_MIN_CONNECTIONS)
        .acquire_timeout(SQLITE_ACQUIRE_TIMEOUT)
        .idle_timeout(SQLITE_IDLE_TIMEOUT)
        .connect_with(options)
        .await?;

    Ok(conn)
}

pub async fn db_drop() {
    let drop = Confirm::new("Drop Database: ")
        .with_default(false)
        .prompt()
        .unwrap_or(false);

    if drop {
        let db_path = DB_PATH.as_ref().unwrap();
        match Sqlite::drop_database(&db_path.to_string_lossy()).await {
            Ok(..) => println!("Successfully dropped DB"),
            Err(e) => eprintln!("{e}"),
        };
    };
}

pub async fn init_globales(conn: &Pool<Sqlite>) -> Result<(), ProcessError> {
    let config = GlobalSettings::new(conn).await;
    GLOBAL_SETTINGS
        .set(config)
        .map_err(|_| "Failed to set global settings")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{database_in, ensure_database_parent};

    #[test]
    fn creates_a_missing_database_parent_directory() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "noar-db-parent-test-{}-{suffix}",
            std::process::id()
        ));
        let database = root.join("nested").join("noar.db");

        ensure_database_parent(&database).unwrap();

        assert!(database.parent().unwrap().is_dir());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_install_from_before_the_rename_keeps_its_database() {
        let root = std::env::temp_dir().join(format!("noar-db-legacy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();

        assert_eq!(database_in(&root), root.join("noar.db"));

        std::fs::write(root.join("ffplayout.db"), "").unwrap();
        assert_eq!(database_in(&root), root.join("ffplayout.db"));

        std::fs::write(root.join("noar.db"), "").unwrap();
        assert_eq!(database_in(&root), root.join("noar.db"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn treats_a_plain_database_file_name_as_the_current_directory() {
        ensure_database_parent(std::path::Path::new("noar.db")).unwrap();
    }
}
