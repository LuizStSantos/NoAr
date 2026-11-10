//! Channel font discovery, immutable upload and authenticated download.

use axum::{
    Json,
    extract::{Path, State},
};
use protect_axum::authorities::AuthDetails;
use serde::Deserialize;

use crate::{
    api::{routes::AuthUser, state::AppState},
    db::{handles, models::Role},
    utils::errors::ServiceError,
};

use super::authorize_channel_access;

/// **Font families the graphics can use**
///
/// The families installed for the engine, for the text layers of templates.
pub async fn get_graphics_fonts(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<String>>, ServiceError> {
    authorize_channel_access(&state, id, &user, &details, false).await?;

    Ok(Json(
        tokio::task::spawn_blocking(noar_engine::available_font_families)
            .await
            .map_err(|_| ServiceError::InternalServerError)?,
    ))
}

/// Uploaded faces available to the authorized channel, without raw font bytes.
pub async fn get_graphics_font_assets(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<crate::db::models::GraphicsFont>>, ServiceError> {
    authorize_channel_access(&state, id, &user, &details, false).await?;

    Ok(Json(handles::select_graphics_fonts(&state.pool, id).await?))
}

#[derive(Deserialize)]
pub struct FontUploadQuery {
    filename: String,
}

pub async fn upload_graphics_font(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    axum::extract::Query(query): axum::extract::Query<FontUploadQuery>,
    body: axum::body::Bytes,
) -> Result<Json<crate::db::models::GraphicsFont>, ServiceError> {
    authorize_channel_access(&state, id, &user, &details, true).await?;

    Ok(Json(
        crate::graphics::fonts::upload(&state.pool, id, &query.filename, body.to_vec()).await?,
    ))
}

pub async fn download_graphics_font(
    State(state): State<AppState>,
    Path((id, font_id)): Path<(i32, String)>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<impl axum::response::IntoResponse, ServiceError> {
    authorize_channel_access(&state, id, &user, &details, false).await?;
    let data = handles::select_graphics_font_data(&state.pool, id, &font_id)
        .await?
        .ok_or_else(|| ServiceError::NotFound("Font not found".to_string()))?;

    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "application/octet-stream"),
            (
                axum::http::header::CACHE_CONTROL,
                "private, max-age=31536000, immutable",
            ),
        ],
        data,
    ))
}
