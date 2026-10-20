use sqlx::SqlitePool;

use crate::{db::models::UpdateSettings, utils::errors::ProcessError};

pub async fn select_update_settings(pool: &SqlitePool) -> Result<UpdateSettings, ProcessError> {
    const QUERY: &str = "SELECT auto_check FROM config_update WHERE id = 1";

    let settings = sqlx::query_as(QUERY).fetch_one(pool).await?;

    Ok(settings)
}

pub async fn update_update_settings(
    pool: &SqlitePool,
    settings: &UpdateSettings,
) -> Result<(), ProcessError> {
    const QUERY: &str = "UPDATE config_update SET auto_check = $1 WHERE id = 1";

    sqlx::query(QUERY)
        .bind(settings.auto_check)
        .execute(pool)
        .await?;

    Ok(())
}
