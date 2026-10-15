use axum::{
    extract::{Path, Query},
    http::HeaderName,
};
use protect_axum::authorities::AuthDetails;

use crate::{
    api::routes::{AuthUser, LogReq, ensure_any_authority},
    db::models::Role,
    utils::{errors::ServiceError, read_log_file},
};

/// ### Log file
///
/// **Read Log File**
///
/// Without `date` it reads today's file. Only complete lines are returned;
/// the `x-log-next` header is the `from` that continues with the lines
/// written afterwards.
///
/// ```BASH
/// curl -X GET http://127.0.0.1:8787/api/log/1?date=2022-06-20
/// -H 'Content-Type: application/json' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_log(
    Path(id): Path<i32>,
    Query(log): Query<LogReq>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<([(HeaderName, String); 1], String), ServiceError> {
    ensure_any_authority(
        &details,
        &[&Role::GlobalAdmin, &Role::ChannelAdmin, &Role::User],
    )?;
    user.ensure_channel_or_admin(id)?;

    let chunk = read_log_file(&id, &log.date, log.timezone, log.download, log.from).await?;

    Ok((
        [(
            HeaderName::from_static("x-log-next"),
            chunk.next.to_string(),
        )],
        chunk.text,
    ))
}
