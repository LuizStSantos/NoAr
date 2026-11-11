//! The history of what users did, for Settings › Activity.
//!
//! Every change, sign-in and external command with a key is recorded after
//! it is answered: who, on which channel, what (a stable action name and
//! what it was done to) and the result. Reads are not. Entries are kept
//! [`ACTIVITY_DAYS`] days.

use std::sync::atomic::{AtomicI64, Ordering};

use axum::{
    body::{Body, HttpBody, to_bytes},
    extract::{FromRequestParts, MatchedPath, RawPathParams, Request, State},
    http::{
        HeaderMap, Method, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use log::warn;
use real::RealIp;
use serde_json::Value;

use crate::{
    api::{
        auth::{decode_jwt, decode_refresh_jwt},
        state::AppState,
    },
    db::handles::{self, ACTIVITY_DAYS, Activity},
};

/// Request bodies read to say what a change was done to. A larger one (a
/// long playlist, an upload) passes unread.
const MAX_READ_BODY: u64 = 1024 * 1024;
/// Longest `target` kept.
const MAX_TARGET: usize = 200;
/// Body fields that name what a change was done to, in this order. A
/// password is never one of them.
const NAMED_FIELDS: [&str; 11] = [
    "username", "name", "title", "date", "source", "target", "template", "command", "control",
    "kind", "folder",
];

/// When old entries were last removed.
static LAST_PRUNE: AtomicI64 = AtomicI64::new(0);

/// The name a request is recorded under, or `None` for what is not
/// recorded: reads (also those sent as POST), token renewals and the event
/// stream.
pub fn action_of(method: &Method, route: &str) -> Option<&'static str> {
    // External commands with a key also come as GET.
    match route {
        "/graphics/{id}/take/{template}" => return Some("external.graphicsTake"),
        "/graphics/{id}/out" | "/graphics/{id}/out/{template}" => {
            return Some("external.graphicsOut");
        }
        "/scte35/{id}/{action}" => return Some("external.break"),
        "/live/{id}/{action}" => return Some("external.live"),
        _ => {}
    }
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return None;
    }

    let action = match (method.as_str(), route) {
        (_, "/auth/refresh" | "/api/generate-uuid") => return None,
        // Reads sent as POST.
        (
            _,
            "/api/file/{id}/browse"
            | "/api/file/{id}/access-token"
            | "/api/playlist/{id}/programs"
            | "/api/playlist/{id}/episodes"
            | "/graphics/{id}/on-air",
        ) => return None,
        ("POST", "/api/file/{id}/sheet" | "/api/file/{id}/logo-marks") => return None,

        ("POST", "/auth/login") => "auth.login",
        ("POST", "/auth/verify") => "auth.verify",
        ("POST", "/auth/logout") => "auth.logout",

        ("POST", "/api/control/{id}/playout") => "player.playout",
        ("PUT", "/api/control/{id}/audio") => "player.audio",
        ("POST", "/api/control/{id}/process") => "player.control",
        ("POST", "/api/control/{id}/scte35/break") => "player.break",
        (_, "/api/control/{id}/scte35/break/{event_id}") => "player.breakCancel",

        ("POST", "/api/playlist/{id}") => "playlist.save",
        ("POST", "/api/playlist/{id}/generate/{date}") => "playlist.generate",
        ("DELETE", "/api/playlist/{id}/{date}") => "playlist.delete",
        (_, "/api/file/{id}/import") => "playlist.import",

        ("POST", "/api/schedule/{id}") => "schedule.create",
        ("PUT", "/api/schedule/{id}/{event_id}") => "schedule.update",
        ("DELETE", "/api/schedule/{id}/{event_id}") => "schedule.delete",
        ("POST", "/api/schedule/{id}/{event_id}/trigger") => "schedule.trigger",
        ("POST", "/api/schedule/{id}/stop") => "schedule.stop",
        ("POST", "/api/schedule/{id}/extend") => "schedule.extend",
        ("POST", "/api/schedule/{id}/graphics") => "schedule.graphicsCreate",
        ("PUT", "/api/schedule/{id}/graphics/{event_id}") => "schedule.graphicsUpdate",
        ("DELETE", "/api/schedule/{id}/graphics/{event_id}") => "schedule.graphicsDelete",
        ("POST", "/api/schedule/{id}/lives") => "schedule.liveCreate",
        ("PUT", "/api/schedule/{id}/lives") => "schedule.liveUpdate",
        ("POST", "/api/schedule/{id}/lives/test") => "schedule.liveTest",
        ("PUT", "/api/schedule/{id}/programs") => "schedule.programSave",

        ("POST", "/api/graphics/{id}/take") => "graphics.take",
        ("POST", "/api/graphics/{id}/out") => "graphics.out",
        ("POST", "/api/graphics/{id}/channel/{template_id}") => "graphics.model",
        ("POST", "/api/graphics/{id}/templates") => "graphics.templateCreate",
        ("PUT", "/api/graphics/{id}/templates/{template_id}") => "graphics.templateUpdate",
        ("DELETE", "/api/graphics/{id}/templates/{template_id}") => "graphics.templateDelete",
        ("PUT", "/api/graphics/{id}/templates/{template_id}/assignment") => {
            "graphics.templateAssign"
        }
        ("POST", "/api/graphics/{id}/font-assets") => "graphics.fontUpload",
        ("POST", "/api/graphics/{id}/sources") => "graphics.sourceCreate",
        ("PUT", "/api/graphics/{id}/sources/{source_id}") => "graphics.sourceUpdate",
        ("DELETE", "/api/graphics/{id}/sources/{source_id}") => "graphics.sourceDelete",
        ("POST", "/api/graphics/{id}/sources/{source_id}/refresh") => "graphics.sourceRefresh",

        ("POST", "/api/file/{id}/create-folder") => "storage.folderCreate",
        ("POST", "/api/file/{id}/rename") => "storage.rename",
        ("POST", "/api/file/{id}/remove") => "storage.remove",
        (_, "/api/file/{id}/upload") => "storage.upload",
        ("PUT", "/api/file/{id}/sheet") => "storage.sheet",
        (_, "/api/file/{id}/commercial") => "storage.commercial",
        (_, "/api/file/{id}/artwork") => "storage.artwork",
        (_, "/api/file/{id}/artwork/remove") => "storage.artworkRemove",
        (_, "/api/file/{id}/photo") => "storage.photo",
        ("PUT", "/api/file/{id}/logo-marks") => "storage.marks",

        ("PUT", "/api/playout/config/{id}") => "settings.channel",
        ("POST", "/api/playout/config/{id}/test-alert") => "settings.testAlert",
        ("POST", "/api/playout/config/{id}/keys/{kind}") => "settings.keys",
        ("PUT", "/api/global") => "settings.global",
        ("POST", "/api/global/test-mail") => "settings.testMail",
        ("POST", "/api/channel") => "settings.channelCreate",
        ("PATCH", "/api/channel/{id}") => "settings.channelUpdate",
        ("DELETE", "/api/channel/{id}") => "settings.channelDelete",

        ("POST", "/api/user") => "user.create",
        ("PUT", "/api/user/{id}") => "user.update",
        ("DELETE", "/api/user/{id}") => "user.delete",

        ("POST", "/api/setup") => "system.setup",
        ("POST", "/api/system/restart") => "system.restart",
        ("POST", "/api/system/update/check") => "system.updateCheck",
        ("PUT", "/api/system/update/settings") => "system.updateSettings",
        ("POST", "/api/system/update/install") => "system.updateInstall",
        ("POST", "/api/system/update/schedule") => "system.updateSchedule",
        ("DELETE", "/api/system/update/schedule") => "system.updateCancel",

        _ => "other",
    };

    Some(action)
}

/// Record the request once it is answered (see the module).
pub async fn activity_layer(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let Some(route) = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_string())
    else {
        return next.run(request).await;
    };
    let Some(action) = action_of(request.method(), &route) else {
        return next.run(request).await;
    };

    let method = request.method().to_string();
    let ip = request
        .extensions()
        .get::<RealIp>()
        .map(|ip| ip.ip().to_string())
        .unwrap_or_default();
    let bearer = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_string);

    // The decoded parts of the path, and a small JSON body for the names in
    // it, passed on as it was.
    let (mut parts, body) = request.into_parts();
    let params: Vec<(String, String)> = RawPathParams::from_request_parts(&mut parts, &())
        .await
        .map(|params| {
            params
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let (body, json) = if reads_body(&parts.headers, &body) {
        match to_bytes(body, MAX_READ_BODY as usize).await {
            Ok(bytes) => {
                let json = serde_json::from_slice::<Value>(&bytes).ok();
                (Body::from(bytes), json)
            }
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    } else {
        (body, None)
    };

    // A user's name, read before a deletion removes it.
    let user_name = match route.as_str() {
        "/api/user/{id}" => match params.iter().find(|(name, _)| name == "id") {
            Some((_, id)) => match id.parse() {
                Ok(id) => handles::select_user(&state.pool, id)
                    .await
                    .ok()
                    .map(|user| user.username),
                Err(_) => None,
            },
            None => None,
        },
        _ => None,
    };

    let response = next.run(Request::from_parts(parts, body)).await;
    let status = response.status();

    // A 401 is a session to renew or a page opened signed out, not a deed.
    if status == StatusCode::UNAUTHORIZED {
        return response;
    }

    let mut entry = Activity {
        at: Utc::now().timestamp(),
        channel_id: channel_of(&route, &params),
        action: action.to_string(),
        target: target_of(&params, json.as_ref(), user_name.as_deref()),
        method,
        route,
        status: i64::from(status.as_u16()),
        ip,
        ..Default::default()
    };

    if let Some(claims) = match bearer {
        Some(token) => decode_jwt(&token).await.ok(),
        None => None,
    } {
        entry.user_id = Some(claims.id);
        entry.username = claims.username;
        entry.role = claims.role.to_string();
    } else if action == "auth.logout" {
        let refresh = json.as_ref().and_then(|body| body["refresh"].as_str());
        if let Some(claims) = match refresh {
            Some(token) => decode_refresh_jwt(token).await.ok(),
            None => None,
        } {
            entry.user_id = Some(claims.id);
            entry.username = claims.username;
            entry.role = claims.role.to_string();
        }
    } else if let Some(name) = json.as_ref().and_then(|body| body["username"].as_str()) {
        // A sign-in: who tried, whether or not it worked.
        entry.username = shorten(name.trim());
    }

    let pool = state.pool.clone();
    tokio::spawn(async move {
        if let Err(error) = handles::insert_activity(&pool, &entry).await {
            warn!("Could not record activity {}: {error}", entry.action);
        }

        // Remove the old entries about once an hour.
        let now = Utc::now().timestamp();
        let last = LAST_PRUNE.load(Ordering::Relaxed);
        if now - last >= 3600
            && LAST_PRUNE
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            && let Err(error) = handles::prune_activity(&pool, now - ACTIVITY_DAYS * 86_400).await
        {
            warn!("Could not remove old activity: {error}");
        }
    });

    response
}

/// A JSON body of known, small size (a chunked one has no known size).
fn reads_body(headers: &HeaderMap, body: &Body) -> bool {
    let json = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    let small = body
        .size_hint()
        .upper()
        .is_some_and(|length| length <= MAX_READ_BODY);

    json && small
}

/// `{id}` is the channel, except on the routes of users.
fn channel_of(route: &str, params: &[(String, String)]) -> Option<i32> {
    if route.starts_with("/api/user/") {
        return None;
    }

    params
        .iter()
        .find(|(name, _)| name == "id")
        .and_then(|(_, value)| value.parse().ok())
}

/// What the request was done to: the names in its body, then the parts of
/// its path other than `{id}` (the channel, or the user).
fn target_of(params: &[(String, String)], body: Option<&Value>, user: Option<&str>) -> String {
    let mut parts: Vec<String> = user.map(str::to_string).into_iter().collect();

    if let Some(body) = body {
        for field in NAMED_FIELDS {
            let text = match &body[field] {
                Value::String(text) => Some(text.as_str()),
                Value::Object(inner) => inner
                    .get("name")
                    .or_else(|| inner.get("title"))
                    .and_then(Value::as_str),
                _ => None,
            };
            if let Some(text) = text.map(str::trim).filter(|text| !text.is_empty())
                && !parts.iter().any(|part| part == text)
            {
                parts.push(text.to_string());
            }
        }
    }
    for (name, value) in params {
        if name == "id" {
            continue;
        }
        let value = if name.ends_with("_id") {
            format!("#{value}")
        } else {
            value.clone()
        };
        if !parts.contains(&value) {
            parts.push(value);
        }
    }

    shorten(&parts.join(" · "))
}

fn shorten(text: &str) -> String {
    match text.char_indices().nth(MAX_TARGET) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn changes_are_named_and_reads_are_not_recorded() {
        let post = Method::POST;
        assert_eq!(
            action_of(&post, "/api/playlist/{id}"),
            Some("playlist.save")
        );
        assert_eq!(
            action_of(&Method::DELETE, "/api/schedule/{id}/{event_id}"),
            Some("schedule.delete")
        );
        assert_eq!(action_of(&post, "/auth/login"), Some("auth.login"));
        // External commands, also as GET.
        assert_eq!(
            action_of(&Method::GET, "/graphics/{id}/take/{template}"),
            Some("external.graphicsTake")
        );
        assert_eq!(
            action_of(&Method::GET, "/live/{id}/{action}"),
            Some("external.live")
        );
        // An unknown change is still recorded.
        assert_eq!(action_of(&Method::PATCH, "/api/new/{id}"), Some("other"));

        assert_eq!(action_of(&Method::GET, "/api/playlist/{id}"), None);
        assert_eq!(action_of(&post, "/api/file/{id}/browse"), None);
        assert_eq!(action_of(&post, "/api/file/{id}/sheet"), None);
        assert_eq!(
            action_of(&Method::PUT, "/api/file/{id}/sheet"),
            Some("storage.sheet")
        );
        assert_eq!(action_of(&post, "/auth/refresh"), None);
        assert_eq!(action_of(&Method::GET, "/graphics/{id}/on-air"), None);
    }

    #[test]
    fn the_channel_and_what_was_changed_come_from_the_path_and_body() {
        let params = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect::<Vec<_>>()
        };
        let day = params(&[("id", "2"), ("date", "2026-11-12")]);
        assert_eq!(channel_of("/api/playlist/{id}/{date}", &day), Some(2));
        assert_eq!(channel_of("/api/user/{id}", &params(&[("id", "7")])), None);
        assert_eq!(target_of(&day, None, None), "2026-11-12");
        assert_eq!(
            target_of(
                &params(&[("id", "1"), ("template_id", "14")]),
                Some(&json!({"template": {"name": "Lower third"}, "password": "secret"})),
                None,
            ),
            "Lower third · #14"
        );
        assert_eq!(
            target_of(
                &params(&[("id", "1"), ("template", "Breaking news")]),
                None,
                None
            ),
            "Breaking news"
        );
        assert_eq!(
            target_of(
                &params(&[("id", "7")]),
                Some(&json!({"username": "maria", "password": "never"})),
                Some("maria"),
            ),
            "maria"
        );
        // A deletion has no body: the name read before it.
        assert_eq!(
            target_of(&params(&[("id", "7")]), None, Some("maria")),
            "maria"
        );
        assert_eq!(shorten(&"a".repeat(300)).chars().count(), MAX_TARGET + 1);
    }
}
