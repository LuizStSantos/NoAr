use std::net::{IpAddr, SocketAddr};

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, Request, Response},
    middleware::Next,
};
use real::{IpExtractor, RealIp};

/// Whether forwarding headers from `peer` can be believed: only a reverse
/// proxy on this host or on a private network may say who the client is.
fn is_trusted_proxy(peer: IpAddr) -> bool {
    match peer {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return is_trusted_proxy(IpAddr::V4(v4));
            }

            ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
        }
    }
}

/// The client address of a request that came in from `peer`.
///
/// A direct client is identified by its socket address, whatever headers it
/// sends; forwarding headers count only when `peer` is a trusted proxy.
pub fn client_ip(peer: Option<IpAddr>, headers: &HeaderMap) -> IpAddr {
    let Some(peer) = peer else {
        return IpAddr::from([127, 0, 0, 1]);
    };

    if !is_trusted_proxy(peer) {
        return peer;
    }

    let headers = headers
        .iter()
        .filter_map(|(name, value)| {
            Some((
                name.as_str().to_lowercase(),
                value.to_str().ok()?.to_string(),
            ))
        })
        .collect::<real::HeaderMap>();

    IpExtractor::default()
        .trust_private_ips(true)
        .extract(&headers, Some(peer.to_string()))
        .unwrap_or(peer)
}

/// Stores the client address as [`RealIp`] for the rate limiter, the request
/// log and the handlers that bind tokens to an address.
pub async fn client_ip_layer(mut req: Request<Body>, next: Next) -> Response<Body> {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip());
    let ip = client_ip(peer, req.headers());

    req.extensions_mut().insert(RealIp(ip));

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn forwarded(ip: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_str(ip).unwrap());

        headers
    }

    #[test]
    fn a_public_client_cannot_choose_its_address() {
        let peer = "203.0.113.9".parse().ok();

        assert_eq!(client_ip(peer, &forwarded("198.51.100.1")), peer.unwrap());
    }

    #[test]
    fn a_local_proxy_passes_the_client_address() {
        for proxy in ["127.0.0.1", "172.18.0.2", "::1", "::ffff:10.0.0.5"] {
            let peer = proxy.parse().ok();

            assert_eq!(
                client_ip(peer, &forwarded("198.51.100.1")),
                "198.51.100.1".parse::<IpAddr>().unwrap(),
                "proxy {proxy}"
            );
        }
    }

    #[test]
    fn a_proxy_without_headers_is_the_client() {
        let peer = "172.18.0.2".parse().ok();

        assert_eq!(client_ip(peer, &HeaderMap::new()), peer.unwrap());
    }
}
