//! Persistent resources used by Graphics models: immutable fonts and live feeds.
//!
//! Feed revisions and refresh sequences guard against stale asynchronous writes;
//! this repository stores bytes/records without fetching or rendering them.

use sqlx::sqlite::SqlitePool;

use crate::utils::errors::ProcessError;

pub async fn select_graphics_fonts(
    pool: &SqlitePool,
    channel_id: i32,
) -> Result<Vec<crate::db::models::GraphicsFont>, ProcessError> {
    Ok(sqlx::query_as("SELECT id, name, filename, family, size FROM graphics_fonts WHERE channel_id = $1 ORDER BY name, id")
        .bind(channel_id).fetch_all(pool).await?)
}

pub async fn select_graphics_font_data(
    pool: &SqlitePool,
    channel_id: i32,
    id: &str,
) -> Result<Option<Vec<u8>>, ProcessError> {
    Ok(
        sqlx::query_scalar("SELECT data FROM graphics_fonts WHERE channel_id = $1 AND id = $2")
            .bind(channel_id)
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn insert_graphics_font(
    pool: &SqlitePool,
    channel_id: i32,
    font: &crate::db::models::GraphicsFont,
    data: &[u8],
) -> Result<(), ProcessError> {
    sqlx::query("INSERT INTO graphics_fonts (id, channel_id, name, filename, family, size, data) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(&font.id).bind(channel_id).bind(&font.name).bind(&font.filename).bind(&font.family).bind(font.size).bind(data).execute(pool).await?;

    Ok(())
}

pub async fn select_graphics_sources(
    pool: &SqlitePool,
    channel: i32,
) -> Result<Vec<crate::db::models::GraphicsDataSource>, ProcessError> {
    Ok(sqlx::query_as(
        "SELECT * FROM graphics_data_sources WHERE channel_id = $1 ORDER BY name, id",
    )
    .bind(channel)
    .fetch_all(pool)
    .await?)
}

pub async fn select_graphics_source(
    pool: &SqlitePool,
    channel: i32,
    id: i32,
) -> Result<Option<crate::db::models::GraphicsDataSource>, ProcessError> {
    Ok(
        sqlx::query_as("SELECT * FROM graphics_data_sources WHERE channel_id = $1 AND id = $2")
            .bind(channel)
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn save_graphics_source(
    pool: &SqlitePool,
    channel: i32,
    id: Option<i32>,
    input: &crate::graphics::sources::SourceInput,
) -> Result<i32, ProcessError> {
    if let Some(id) = id {
        let result = sqlx::query("UPDATE graphics_data_sources SET name=$3,kind=$4,url=$5,interval=$6,records_path=$7,document=$8,revision=revision+1 WHERE channel_id=$1 AND id=$2 AND revision=$9")
            .bind(channel).bind(id).bind(&input.name).bind(&input.kind).bind(&input.url).bind(input.interval).bind(&input.records_path).bind(&input.document).bind(input.revision).execute(pool).await?;

        if result.rows_affected() == 0 {
            return Err(ProcessError::Custom(
                "Data source changed or was deleted; reload before saving".to_string(),
            ));
        }

        return Ok(id);
    }

    Ok(sqlx::query_scalar("INSERT INTO graphics_data_sources (channel_id,name,kind,url,interval,records_path,document) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id")
        .bind(channel).bind(&input.name).bind(&input.kind).bind(&input.url).bind(input.interval).bind(&input.records_path).bind(&input.document).fetch_one(pool).await?)
}

pub async fn begin_graphics_source_refresh(
    pool: &SqlitePool,
    channel: i32,
    id: i32,
) -> Result<Option<crate::db::models::GraphicsDataSource>, ProcessError> {
    Ok(sqlx::query_as("UPDATE graphics_data_sources SET fetch_seq=fetch_seq+1,last_attempt=$3 WHERE channel_id=$1 AND id=$2 RETURNING *")
        .bind(channel).bind(id).bind(chrono::Utc::now().timestamp_millis()).fetch_optional(pool).await?)
}

pub async fn finish_graphics_source_refresh(
    pool: &SqlitePool,
    source: &crate::db::models::GraphicsDataSource,
    result: Result<crate::graphics::sources::DataRows, String>,
) -> Result<(), ProcessError> {
    match result {
        Ok(rows) => {
            let data = serde_json::to_string(&rows)
                .map_err(|error| ProcessError::Custom(error.to_string()))?;
            sqlx::query("UPDATE graphics_data_sources SET data=$5,last_success=$6,error=NULL WHERE channel_id=$1 AND id=$2 AND revision=$3 AND fetch_seq=$4")
                .bind(source.channel_id).bind(source.id).bind(source.revision).bind(source.fetch_seq).bind(data).bind(chrono::Utc::now().timestamp_millis()).execute(pool).await?;
        }
        Err(error) => {
            sqlx::query("UPDATE graphics_data_sources SET error=$5 WHERE channel_id=$1 AND id=$2 AND revision=$3 AND fetch_seq=$4")
            .bind(source.channel_id).bind(source.id).bind(source.revision).bind(source.fetch_seq).bind(error).execute(pool).await?;
        }
    }

    Ok(())
}

pub async fn delete_graphics_source(
    pool: &SqlitePool,
    channel: i32,
    id: i32,
) -> Result<bool, ProcessError> {
    Ok(
        sqlx::query("DELETE FROM graphics_data_sources WHERE channel_id=$1 AND id=$2")
            .bind(channel)
            .bind(id)
            .execute(pool)
            .await?
            .rows_affected()
            > 0,
    )
}
