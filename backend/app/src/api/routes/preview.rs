use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
};
use protect_axum::authorities::AuthDetails;
use serde::Deserialize;
use tokio_stream::{StreamExt, wrappers::BroadcastStream};

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::models::Role,
    utils::errors::ServiceError,
};

/// **Live preview**
///
/// The channel output as a live MPEG-TS stream, about a second behind the
/// air, whatever the output is (RTMP, SRT, HLS). Nothing is sent while the
/// playout is stopped.
///
/// ```BASH
/// curl -N http://127.0.0.1:8787/api/preview/1 -H 'Authorization: Bearer <TOKEN>' | ffplay -
/// ```
///
/// `?quality=light` sends a smaller copy instead (640×360, 15 frames per
/// second, about 0.6 Mb/s with the sound), for slow connections. It is
/// encoded only while someone watches it.
#[derive(Debug, Deserialize)]
pub struct PreviewQuery {
    /// `light` for the smaller copy; anything else is the full preview.
    quality: Option<String>,
}

pub async fn get_preview(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Query(query): Query<PreviewQuery>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Response, ServiceError> {
    ensure_any_authority(
        &details,
        &[&Role::GlobalAdmin, &Role::ChannelAdmin, &Role::User],
    )?;
    user.ensure_channel_or_admin(id)?;

    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::BadRequest(format!("Channel {id} not found!")))?;

    // A viewer that falls behind skips the bytes it missed; the player picks
    // up again at the next packet boundary.
    let receiver = if query.quality.as_deref() == Some("light") {
        manager.preview.subscribe_light()
    } else {
        manager.preview.subscribe()
    };
    let stream = BroadcastStream::new(receiver)
        .filter_map(|bytes| bytes.ok().map(Ok::<_, std::convert::Infallible>));

    Ok((
        [
            (header::CONTENT_TYPE, "video/mp2t"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}
