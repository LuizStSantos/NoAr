//! Which parts of NoAr an operator may use.
//!
//! A global admin chooses, per operator, the areas (Player, Playlist,
//! Scheduler, Graphics, Storage, Report, Log) they may use. The interface
//! hides the pages of the other areas; this guard refuses their requests, so
//! a page left out cannot be used through the API either. It also refuses a
//! token that no longer matches its saved user (see [`account_guard`]).
//!
//! What changes the channel is guarded, and the Report and Log, which only
//! their pages read. Other reads stay open to everyone on the channel: the
//! Player shows the playlist, the Scheduler and Playlist browse the storage,
//! and every page shows what is on air.

use axum::{
    Json,
    extract::{MatchedPath, Request, State},
    http::{Method, StatusCode, header::AUTHORIZATION},
    middleware::Next,
    response::{IntoResponse, Response},
};
use log::error;

use crate::{
    api::{auth::decode_jwt, state::AppState},
    db::{
        handles,
        models::{Area, Role},
    },
};

use Area::*;

/// The areas that allow a request, any one of them: an empty list means
/// every user on the channel may make it (other checks still apply).
pub fn required_areas(method: &Method, path: &str) -> &'static [Area] {
    let path = path.strip_prefix("/api").unwrap_or(path);
    let reads = method == Method::GET;

    match path {
        "/report/{id}" => &[Report],
        "/log/{id}" => &[Log],
        _ if reads => &[],
        // The transport, the volume and the breaks the Player shows.
        "/control/{id}/process" | "/control/{id}/playout" | "/control/{id}/audio" => &[Player],
        "/control/{id}/scte35/break" | "/control/{id}/scte35/break/{event_id}" => {
            &[Player, Schedule]
        }
        "/playlist/{id}"
        | "/playlist/{id}/generate/{date}"
        | "/playlist/{id}/{date}"
        | "/file/{id}/import" => &[Playlist],
        // The Player ends or extends the live on air too.
        "/schedule/{id}/stop" | "/schedule/{id}/extend" => &[Player, Schedule],
        // Lives and assembled programs are made in Storage as well.
        "/schedule/{id}/lives" | "/schedule/{id}/lives/test" | "/schedule/{id}/programs" => {
            &[Schedule, Storage]
        }
        path if path.starts_with("/schedule/{id}") => &[Schedule],
        "/graphics/{id}/take" | "/graphics/{id}/out" | "/graphics/{id}/channel/{template_id}" => {
            &[Graphics]
        }
        "/file/{id}/create-folder"
        | "/file/{id}/rename"
        | "/file/{id}/remove"
        | "/file/{id}/upload"
        | "/file/{id}/sheet"
        | "/file/{id}/commercial"
        | "/file/{id}/artwork"
        | "/file/{id}/artwork/remove"
        | "/file/{id}/photo"
        | "/file/{id}/logo-marks"
            // Reading a sheet or the marks of a video is a POST too.
            if !(method == Method::POST && matches!(path, "/file/{id}/sheet" | "/file/{id}/logo-marks")) =>
        {
            &[Storage]
        }
        _ => &[],
    }
}

/// Whether a user with `areas` (`None`: every area) may make a request that
/// needs one of `required`.
pub fn allowed(areas: Option<&[Area]>, required: &[Area]) -> bool {
    required.is_empty()
        || areas.is_none_or(|areas| required.iter().any(|area| areas.contains(area)))
}

/// Check every signed-in request against the saved user.
///
/// A token issued before its user's role, channels or pages changed, or
/// before the user was deleted, is refused with 401: the interface renews it
/// at once and so follows the change (a renewal reads the saved user). Then
/// an operator's request for an area they were not given is refused with
/// 403. Requests without a valid token pass, for the route itself to refuse.
pub async fn account_guard(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    let Some(claims) = (match token {
        Some(token) => decode_jwt(token).await.ok(),
        None => None,
    }) else {
        return next.run(request).await;
    };

    // The saved user, not the token's copy: a change applies at once.
    let (user, role) = match handles::select_user_access(&state.pool, claims.id).await {
        Ok(Some(account)) => account,
        Ok(None) => return renew("This user no longer exists"),
        Err(error) => {
            error!("Could not read user {}: {error}", claims.id);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if !claims.matches_account(&user, &role) {
        return renew("Your user was changed; renew the session");
    }

    if role != Role::User {
        return next.run(request).await;
    }

    let required = request
        .extensions()
        .get::<MatchedPath>()
        .map_or(&[][..], |path| {
            required_areas(request.method(), path.as_str())
        });

    if allowed(user.area_list().as_deref(), required) {
        next.run(request).await
    } else {
        (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "detail": "Your user cannot use this part of NoAr" })),
        )
            .into_response()
    }
}

/// A 401 that makes the interface renew its token, or sign out when the
/// renewal is refused too.
fn renew(detail: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "detail": detail })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_that_change_the_channel_need_their_area() {
        let post = Method::POST;
        assert_eq!(required_areas(&post, "/api/playlist/{id}"), &[Playlist]);
        assert_eq!(
            required_areas(&Method::DELETE, "/api/playlist/{id}/{date}"),
            &[Playlist]
        );
        assert_eq!(
            required_areas(&post, "/api/control/{id}/process"),
            &[Player]
        );
        assert_eq!(required_areas(&post, "/api/schedule/{id}"), &[Schedule]);
        assert_eq!(
            required_areas(&Method::PUT, "/api/schedule/{id}/{event_id}"),
            &[Schedule]
        );
        assert_eq!(
            required_areas(&post, "/api/schedule/{id}/stop"),
            &[Player, Schedule]
        );
        assert_eq!(
            required_areas(&Method::PUT, "/api/schedule/{id}/lives"),
            &[Schedule, Storage]
        );
        assert_eq!(
            required_areas(&post, "/api/graphics/{id}/take"),
            &[Graphics]
        );
        assert_eq!(
            required_areas(&Method::PUT, "/api/file/{id}/upload"),
            &[Storage]
        );
        assert_eq!(
            required_areas(&Method::PUT, "/api/file/{id}/sheet"),
            &[Storage]
        );
        assert_eq!(required_areas(&post, "/api/file/{id}/remove"), &[Storage]);
        assert_eq!(
            required_areas(&Method::PUT, "/api/file/{id}/import"),
            &[Playlist]
        );
    }

    #[test]
    fn the_report_and_log_need_their_area_and_other_reads_do_not() {
        assert_eq!(required_areas(&Method::GET, "/api/report/{id}"), &[Report]);
        assert_eq!(required_areas(&Method::GET, "/api/log/{id}"), &[Log]);
        // Pages share these reads: the Player shows the playlist, the
        // Scheduler and Playlist browse the storage.
        assert!(required_areas(&Method::GET, "/api/playlist/{id}").is_empty());
        assert!(required_areas(&Method::GET, "/api/schedule/{id}").is_empty());
        assert!(required_areas(&Method::POST, "/api/file/{id}/browse").is_empty());
        assert!(required_areas(&Method::POST, "/api/file/{id}/sheet").is_empty());
        assert!(required_areas(&Method::POST, "/api/file/{id}/logo-marks").is_empty());
        assert!(required_areas(&Method::POST, "/api/playlist/{id}/programs").is_empty());
        assert!(required_areas(&Method::GET, "/api/control/{id}/media/current").is_empty());
    }

    #[test]
    fn every_area_by_default_and_one_matching_area_is_enough() {
        assert!(allowed(None, &[Storage]));
        assert!(allowed(Some(&[]), &[]));
        assert!(allowed(Some(&[Player]), &[Player, Schedule]));
        assert!(!allowed(Some(&[Playlist]), &[Player, Schedule]));
        assert!(!allowed(Some(&[]), &[Report]));
    }
}
