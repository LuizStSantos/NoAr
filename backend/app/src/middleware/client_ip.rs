use std::{
    net::{IpAddr, SocketAddr},
    sync::LazyLock,
};

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, Request, Response},
    middleware::Next,
};
use log::warn;
use real::{IpExtractor, RealIp};

/// The proxies named in `NOAR_TRUSTED_PROXIES` (addresses or networks such as
/// `172.18.0.0/16`, separated by commas or spaces); `None` when it is unset.
static TRUSTED_PROXIES: LazyLock<Option<Vec<Network>>> = LazyLock::new(|| {
    std::env::var("NOAR_TRUSTED_PROXIES")
        .ok()
        .filter(|text| !text.trim().is_empty())
        .map(|text| parse_networks(&text))
});

/// An address and how many of its leading bits a peer must share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Network {
    address: IpAddr,
    prefix: u32,
}

impl Network {
    fn contains(self, ip: IpAddr) -> bool {
        match (self.address, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = u32::MAX.checked_shl(32 - self.prefix).unwrap_or(0);
                u32::from(net) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = u128::MAX.checked_shl(128 - self.prefix).unwrap_or(0);
                u128::from(net) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

/// Read `10.0.0.5, 172.18.0.0/16 fd00::/8`; a wrong entry is left out with
/// a warning, so a typo does not stop NoAr.
fn parse_networks(text: &str) -> Vec<Network> {
    let mut networks = vec![];

    for entry in text
        .split([',', ' ', '\t', '\n'])
        .filter(|entry| !entry.is_empty())
    {
        match parse_network(entry) {
            Some(network) => networks.push(network),
            None => warn!("NOAR_TRUSTED_PROXIES: {entry:?} is not an address or network; left out"),
        }
    }

    networks
}

fn parse_network(entry: &str) -> Option<Network> {
    let (address, prefix) = entry.split_once('/').unwrap_or((entry, ""));
    let address = unmap(address.parse::<IpAddr>().ok()?);
    let bits = if address.is_ipv4() { 32 } else { 128 };
    let prefix = if prefix.is_empty() {
        bits
    } else {
        prefix
            .parse::<u32>()
            .ok()
            .filter(|prefix| *prefix <= bits)?
    };

    Some(Network { address, prefix })
}

/// An IPv4 address written as IPv6 (`::ffff:10.0.0.5`) is that IPv4 address.
fn unmap(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    }
}

/// Whether forwarding headers from `peer` can be believed. A proxy on this
/// host always may say who the client is. With `NOAR_TRUSTED_PROXIES` set,
/// only the proxies it names may too; without it, any address of a private
/// network may, so a proxy in another container works unconfigured (but a
/// device on that network can also choose the address it is counted as).
fn is_trusted_proxy(peer: IpAddr, trusted: Option<&[Network]>) -> bool {
    let peer = unmap(peer);

    if peer.is_loopback() {
        return true;
    }
    if let Some(trusted) = trusted {
        return trusted.iter().any(|network| network.contains(peer));
    }

    match peer {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}

/// The client address of a request that came in from `peer`.
///
/// A direct client is identified by its socket address, whatever headers it
/// sends; forwarding headers count only when `peer` is a trusted proxy.
pub fn client_ip(peer: Option<IpAddr>, headers: &HeaderMap) -> IpAddr {
    client_ip_with(peer, headers, TRUSTED_PROXIES.as_deref())
}

fn client_ip_with(
    peer: Option<IpAddr>,
    headers: &HeaderMap,
    trusted: Option<&[Network]>,
) -> IpAddr {
    let Some(peer) = peer else {
        return IpAddr::from([127, 0, 0, 1]);
    };

    if !is_trusted_proxy(peer, trusted) {
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
    fn named_proxies_replace_the_private_networks() {
        let trusted = parse_networks("172.18.0.0/16, 10.0.0.5 fd00::/8 nonsense 10.1.0.0/40");
        assert_eq!(trusted.len(), 3);
        let client = |peer: &str| {
            client_ip_with(
                peer.parse().ok(),
                &forwarded("198.51.100.1"),
                Some(&trusted),
            )
        };
        let forwarded_client = "198.51.100.1".parse::<IpAddr>().unwrap();

        for proxy in [
            "127.0.0.1",
            "172.18.4.2",
            "10.0.0.5",
            "::ffff:10.0.0.5",
            "fd12::1",
            "::1",
        ] {
            assert_eq!(client(proxy), forwarded_client, "proxy {proxy}");
        }
        // Another device on the private network is counted as itself.
        for device in ["192.168.1.50", "10.0.0.6", "172.19.0.2", "fe80::1"] {
            assert_eq!(
                client(device),
                device.parse::<IpAddr>().unwrap(),
                "device {device}"
            );
        }
    }

    #[test]
    fn a_proxy_without_headers_is_the_client() {
        let peer = "172.18.0.2".parse().ok();

        assert_eq!(client_ip(peer, &HeaderMap::new()), peer.unwrap());
    }
}
