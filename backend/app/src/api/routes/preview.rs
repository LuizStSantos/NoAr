use axum::{
    body::Body,
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
};
use protect_axum::authorities::AuthDetails;
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
pub async fn get_preview(
    State(state): State<AppState>,
    Path(id): Path<i32>,
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
    let stream = BroadcastStream::new(manager.preview.subscribe())
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
