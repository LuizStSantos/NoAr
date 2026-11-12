//! Security headers of the web interface.
//!
//! The interface keeps its session in the browser's storage, so a script
//! that got into one of its pages could take the whole session. Its pages are
//! therefore served with a content security policy that only runs the
//! scripts NoAr serves itself: nothing inline, nothing evaluated, no plugins.
//! What the pages show stays open, since artwork, previews and live sources
//! may come from other servers.
//!
//! The headers go on the interface only. API answers, stored files and HLS
//! have their own (see `api::routes::file`).

use std::sync::LazyLock;

use axum::{
    body::Body,
    http::{
        HeaderValue, Request, Response,
        header::{CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS},
    },
    middleware::Next,
};
use log::warn;

/// What a page of the interface may load and run.
const POLICY: &[&str] = &[
    "default-src 'self'",
    // Only the scripts NoAr serves: nothing inline and no eval.
    "script-src 'self'",
    // Vue, the players and the PDF export put styles on elements.
    "style-src 'self' 'unsafe-inline'",
    // Artwork, logos, previews and live sources may be on other servers.
    "img-src 'self' data: blob: http: https:",
    "media-src 'self' blob: http: https:",
    "connect-src 'self' data: blob: http: https:",
    "font-src 'self' data: blob:",
    // The players decode in workers made from blobs. Browsers without
    // worker-src read child-src.
    "worker-src 'self' blob:",
    "child-src 'self' blob:",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
];

/// Who may show the interface inside a frame, from `NOAR_FRAME_ANCESTORS`
/// (`'self'`, `'none'` or the addresses of the pages that embed it). Unset,
/// any page may, as before.
static FRAME_ANCESTORS: LazyLock<Option<String>> = LazyLock::new(|| {
    let text = std::env::var("NOAR_FRAME_ANCESTORS").ok()?;

    frame_ancestors(&text)
});

static CONTENT_POLICY: LazyLock<Option<HeaderValue>> = LazyLock::new(|| {
    HeaderValue::from_str(&content_security_policy(FRAME_ANCESTORS.as_deref())).ok()
});

/// Read `'self' https://panel.example.org`: sources separated by spaces or
/// commas. One that is not a source is left out with a warning, so a typo
/// can neither stop NoAr nor put anything else into the header.
fn frame_ancestors(text: &str) -> Option<String> {
    let mut sources = vec![];

    for entry in text
        .split([',', ' ', '\t', '\n'])
        .filter(|entry| !entry.is_empty())
    {
        if is_frame_source(entry) {
            sources.push(entry);
        } else {
            warn!("NOAR_FRAME_ANCESTORS: {entry:?} is not a source; left out");
        }
    }

    (!sources.is_empty()).then(|| sources.join(" "))
}

/// `'self'`, `'none'`, `*`, a scheme (`https:`) or a host with an optional
/// scheme and port (`https://panel.example.org:8443`, `*.example.org`).
fn is_frame_source(entry: &str) -> bool {
    if matches!(entry, "'self'" | "'none'" | "*" | "http:" | "https:") {
        return true;
    }

    let host = entry
        .strip_prefix("https://")
        .or_else(|| entry.strip_prefix("http://"))
        .unwrap_or(entry);
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) => (name, Some(port)),
        None => (host, None),
    };
    let name_is_valid = !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-.*".contains(character));
    let port_is_valid = port.is_none_or(|port| {
        port == "*" || (!port.is_empty() && port.chars().all(|digit| digit.is_ascii_digit()))
    });

    name_is_valid && port_is_valid
}

/// The policy of a page, with who may frame it when that is restricted.
pub fn content_security_policy(frame_ancestors: Option<&str>) -> String {
    let mut policy = POLICY.join("; ");

    if let Some(sources) = frame_ancestors {
        policy.push_str("; frame-ancestors ");
        policy.push_str(sources);
    }

    policy
}

/// Add the security headers to what the interface serves: every file says
/// what it is, and a page also says what it may load and run.
pub async fn interface_headers(request: Request<Body>, next: Next) -> Response<Body> {
    let mut response = next.run(request).await;
    let is_page = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|content_type| content_type.starts_with("text/html"));
    let headers = response.headers_mut();

    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("same-origin"));

    if is_page && let Some(policy) = CONTENT_POLICY.as_ref() {
        headers.insert(CONTENT_SECURITY_POLICY, policy.clone());
    }

    response
}

#[cfg(test)]
mod tests {
    use axum::{Router, http::StatusCode, middleware, response::IntoResponse, routing::get};
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn a_page_runs_only_the_scripts_noar_serves() {
        let policy = content_security_policy(None);

        assert!(policy.contains("script-src 'self';"));
        assert!(policy.contains("object-src 'none'"));
        assert!(policy.contains("base-uri 'self'"));
        // No way back in for an injected script.
        assert!(!policy.contains("'unsafe-eval'"));
        assert!(
            !policy
                .split("; ")
                .any(|directive| directive.starts_with("script-src")
                    && directive.contains("'unsafe-inline'"))
        );
        assert!(
            !policy
                .split("; ")
                .any(|directive| directive.starts_with("default-src")
                    && directive != "default-src 'self'")
        );
    }

    #[test]
    fn framing_is_open_unless_it_is_restricted() {
        assert!(!content_security_policy(None).contains("frame-ancestors"));
        assert!(content_security_policy(Some("'self'")).ends_with("; frame-ancestors 'self'"));
    }

    #[test]
    fn frame_ancestors_keep_sources_and_leave_out_anything_else() {
        assert_eq!(frame_ancestors("'self'").as_deref(), Some("'self'"));
        assert_eq!(
            frame_ancestors("'self', https://panel.example.org:8443 *.example.org").as_deref(),
            Some("'self' https://panel.example.org:8443 *.example.org")
        );
        // Nothing that could end the directive or the header gets through.
        assert_eq!(frame_ancestors("'none';script-src *").as_deref(), Some("*"));
        assert_eq!(frame_ancestors("https://a.example/path\r\n"), None);
        assert_eq!(frame_ancestors("panel.example.org:"), None);
        assert_eq!(frame_ancestors("  "), None);
    }

    #[tokio::test]
    async fn pages_get_the_policy_and_every_file_says_what_it_is() {
        let page = || async { ([(CONTENT_TYPE, "text/html")], "<!DOCTYPE html>").into_response() };
        let script = || async { ([(CONTENT_TYPE, "text/javascript")], "void 0").into_response() };
        let app = Router::new()
            .route("/", get(page))
            .route("/assets/app.js", get(script))
            .layer(middleware::from_fn(interface_headers));
        let fetch = |uri: &'static str| {
            let app = app.clone();

            async move {
                app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                    .await
                    .unwrap()
            }
        };

        let page = fetch("/").await;
        assert_eq!(page.status(), StatusCode::OK);
        assert_eq!(page.headers()[X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(page.headers()[REFERRER_POLICY], "same-origin");
        assert!(
            page.headers()[CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .starts_with("default-src 'self'; script-src 'self';")
        );

        let script = fetch("/assets/app.js").await;
        assert_eq!(script.headers()[X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(!script.headers().contains_key(CONTENT_SECURITY_POLICY));
    }
}
