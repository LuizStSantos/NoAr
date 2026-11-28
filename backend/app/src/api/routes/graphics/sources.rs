//! Data-source HTTP adapters; provider fetching and validation stay in the service.

use axum::{
    Json,
    extract::{Path, State},
};
use protect_axum::authorities::AuthDetails;

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::{handles, models::Role},
    utils::errors::ServiceError,
};

use super::authorize_channel_access;

pub async fn get_graphics_sources(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<crate::db::models::GraphicsDataSource>>, ServiceError> {
    authorize_channel_access(&state, id, &user, &details, false).await?;

    let mut sources = handles::select_graphics_sources(&state.pool, id).await?;

    if ensure_any_authority(&details, &[&Role::GlobalAdmin, &Role::ChannelAdmin]).is_err() {
        for source in &mut sources {
            source.url.clear();
        }
    }

    Ok(Json(sources))
}

pub async fn add_graphics_source(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(mut input): Json<crate::graphics::sources::SourceInput>,
) -> Result<Json<crate::db::models::GraphicsDataSource>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, true).await?;
    crate::graphics::sources::validate_input(&mut input)?;

    if handles::select_graphics_sources(&state.pool, id)
        .await?
        .len()
        >= 16
    {
        return Err(ServiceError::Conflict(
            "At most 16 data sources per channel".to_string(),
        ));
    }

    let source_id = handles::save_graphics_source(&state.pool, id, None, &input).await?;
    crate::graphics::sources::refresh(&manager, source_id).await?;

    Ok(Json(
        handles::select_graphics_source(&state.pool, id, source_id)
            .await?
            .ok_or_else(|| ServiceError::NotFound("Data source not found".to_string()))?,
    ))
}

pub async fn update_graphics_source(
    State(state): State<AppState>,
    Path((id, source_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(mut input): Json<crate::graphics::sources::SourceInput>,
) -> Result<Json<crate::db::models::GraphicsDataSource>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, true).await?;
    let old = handles::select_graphics_source(&state.pool, id, source_id)
        .await?
        .ok_or_else(|| ServiceError::NotFound("Data source not found".to_string()))?;

    if input.document.is_empty() && input.url.is_empty() && input.kind == "csv" {
        input.document = old.document;
    }
    crate::graphics::sources::validate_input(&mut input)?;
    handles::save_graphics_source(&state.pool, id, Some(source_id), &input)
        .await
        .map_err(|error| ServiceError::Conflict(error.to_string()))?;
    crate::graphics::sources::refresh(&manager, source_id).await?;

    Ok(Json(
        handles::select_graphics_source(&state.pool, id, source_id)
            .await?
            .ok_or_else(|| ServiceError::NotFound("Data source not found".to_string()))?,
    ))
}

pub async fn refresh_graphics_source(
    State(state): State<AppState>,
    Path((id, source_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<crate::db::models::GraphicsDataSource>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, true).await?;
    crate::graphics::sources::refresh(&manager, source_id).await?;

    Ok(Json(
        handles::select_graphics_source(&state.pool, id, source_id)
            .await?
            .ok_or_else(|| ServiceError::NotFound("Data source not found".to_string()))?,
    ))
}

pub async fn delete_graphics_source(
    State(state): State<AppState>,
    Path((id, source_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<&'static str>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, true).await?;
    let _edit = manager.graphics_takes.begin_edit().await;
    let templates = handles::select_graphics_templates(&state.pool, id).await?;
    let used: Vec<_> = templates
        .iter()
        .filter(|template| {
            crate::graphics::sources::bindings(template)
                .values()
                .any(|field| field.source_id == source_id)
        })
        .map(|template| template.name.clone())
        .collect();

    if !used.is_empty() {
        return Err(ServiceError::Conflict(format!(
            "Unlink this source from: {}",
            used.join(", ")
        )));
    }

    if !handles::delete_graphics_source(&state.pool, id, source_id).await? {
        return Err(ServiceError::NotFound("Data source not found".to_string()));
    }
    manager
        .config
        .write()
        .await
        .graphics_data
        .remove(&source_id);

    Ok(Json("Success"))
}
