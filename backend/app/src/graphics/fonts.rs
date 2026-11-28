//! Immutable, channel-owned font assets shared by the browser and engine.
use crate::{
    db::{
        handles,
        models::{GraphicsFont, GraphicsTemplate},
    },
    utils::errors::ServiceError,
};

pub const MAX_FONT_BYTES: usize = 10 * 1024 * 1024;

pub async fn upload(
    pool: &sqlx::SqlitePool,
    channel: i32,
    filename: &str,
    data: Vec<u8>,
) -> Result<GraphicsFont, ServiceError> {
    let filename = sanitize_filename::sanitize(filename);
    let extension = std::path::Path::new(&filename)
        .extension()
        .and_then(|part| part.to_str())
        .unwrap_or_default();

    if !["ttf", "otf"].contains(&extension.to_ascii_lowercase().as_str())
        || data.is_empty()
        || data.len() > MAX_FONT_BYTES
    {
        return Err(ServiceError::BadRequest(
            "Upload a TTF/OTF font up to 10 MiB".to_string(),
        ));
    }

    if handles::select_graphics_fonts(pool, channel).await?.len() >= 32 {
        return Err(ServiceError::Conflict(
            "At most 32 uploaded fonts per channel".to_string(),
        ));
    }

    let checked = data.clone();
    let name = tokio::task::spawn_blocking(move || noar_engine::uploaded_font_name(&checked))
        .await
        .map_err(|_| ServiceError::InternalServerError)?
        .map_err(|error| ServiceError::BadRequest(error.to_string()))?;
    let id = uuid::Uuid::new_v4().to_string();
    let asset = GraphicsFont {
        family: format!("NoArFont_{}", id.replace('-', "")),
        id,
        name,
        filename,
        size: data.len() as i64,
    };
    handles::insert_graphics_font(pool, channel, &asset, &data).await?;
    let family = asset.family.clone();
    tokio::task::spawn_blocking(move || noar_engine::register_uploaded_font(&family, data))
        .await
        .map_err(|_| ServiceError::InternalServerError)?
        .map_err(|error| ServiceError::Conflict(error.to_string()))?;

    Ok(asset)
}

pub async fn load(pool: &sqlx::SqlitePool, channel: i32) -> Result<(), ServiceError> {
    for asset in handles::select_graphics_fonts(pool, channel).await? {
        if let Some(data) = handles::select_graphics_font_data(pool, channel, &asset.id).await? {
            let family = asset.family.clone();
            let result = tokio::task::spawn_blocking(move || {
                noar_engine::register_uploaded_font(&family, data)
            })
            .await
            .map_err(|_| ServiceError::InternalServerError)?;

            if let Err(error) = result {
                log::warn!(channel=channel; "Uploaded font {} is unavailable: {error}", asset.id);
            }
        }
    }

    Ok(())
}

pub async fn validate(
    pool: &sqlx::SqlitePool,
    template: &GraphicsTemplate,
) -> Result<(), ServiceError> {
    let assets = handles::select_graphics_fonts(pool, template.channel_id).await?;

    for family in template
        .layers
        .iter()
        .filter_map(|layer| layer.font_family.as_deref())
    {
        if family.starts_with("NoArFont_") && !assets.iter().any(|asset| asset.family == family) {
            return Err(ServiceError::BadRequest(
                "The uploaded font is not available on this channel".to_string(),
            ));
        }
    }

    Ok(())
}
