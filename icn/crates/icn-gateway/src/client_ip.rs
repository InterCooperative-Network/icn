//! Canonical client-IP derivation for the gateway (#2567).
//!
//! # Trust model
//!
//! Forwarded client identity is an **untrusted request claim** unless the immediate transport
//! peer is explicitly trusted to assert forwarding metadata. If the immediate peer is not
//! trusted, the socket peer IP is the client IP.
//!
//! This is the same allowlist `api::sessions::get_gateway_url` uses to gate `X-Forwarded-*`
//! for QR-login URL derivation; both read it through [`is_trusted_proxy`] so the gateway has
//! exactly one trusted-proxy model.
//!
//! # Configuration
//!
//! `TRUSTED_PROXY_IPS` is a comma-separated list of **exact** proxy addresses (no CIDR):
//!
//! ```text
//! TRUSTED_PROXY_IPS="10.42.0.1,::1"
//! ```
//!
//! - unset — loopback only (`127.0.0.1`, `::1`)
//! - set and empty — trust nobody; every request is keyed on its socket peer
//! - unparseable entries are ignored (they cannot match a kernel-supplied peer address)
//!
//! Addresses are compared as parsed [`IpAddr`]s with IPv4-mapped IPv6 unwrapped, so a peer
//! arriving as `::ffff:10.42.0.1` on a dual-stack listener matches a `10.42.0.1` entry. Both
//! sides of the comparison come from the same normalization, so this narrows nothing: the
//! mapped form denotes the same host. It also keeps the rate-limit key from splitting across
//! two spellings of one address, the class fixed in #2557 / #2563.
//!
//! # Deployment note
//!
//! A trusted proxy must *overwrite or append to* `X-Forwarded-For` itself. A proxy that
//! forwards the client's header verbatim launders the claim through the trust gate: the
//! gateway cannot tell a proxy-asserted hop from a caller-supplied one.

use std::net::IpAddr;

use actix_web::HttpRequest;

/// Header carrying the forwarding chain. `X-Real-IP` and RFC 7239 `Forwarded` are
/// deliberately not consulted — neither is set by any proxy config in this repository, and
/// reading them would widen the attacker-controlled input surface for no deployment benefit.
const FORWARDED_FOR: &str = "x-forwarded-for";

/// Environment variable holding the trusted-proxy allowlist.
const TRUSTED_PROXY_ENV: &str = "TRUSTED_PROXY_IPS";

/// Key used when a request has no transport peer at all (synthetic/test requests).
const UNKNOWN_CLIENT: &str = "unknown";

/// Unwrap IPv4-mapped IPv6 (`::ffff:a.b.c.d`) to its IPv4 form.
///
/// Uses `to_ipv4_mapped`, not `to_ipv4`: the latter also converts IPv4-*compatible* addresses
/// (`::a.b.c.d`), which are deprecated and would let two distinct notions of an address
/// collapse together.
fn canonicalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    }
}

/// Parse the configured trusted-proxy allowlist.
fn trusted_proxies() -> Vec<IpAddr> {
    let Ok(configured) = std::env::var(TRUSTED_PROXY_ENV) else {
        // Default: only a proxy on this host may assert forwarding metadata.
        return vec![
            IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
        ];
    };

    configured
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| match entry.parse::<IpAddr>() {
            Ok(ip) => Some(canonicalize(ip)),
            Err(_) => {
                tracing::debug!(
                    entry = %entry,
                    "Ignoring unparseable {TRUSTED_PROXY_ENV} entry; it cannot match a peer address"
                );
                None
            }
        })
        .collect()
}

/// Whether `ip` is allowed to assert forwarding metadata.
pub(crate) fn is_trusted_proxy(ip: IpAddr) -> bool {
    let ip = canonicalize(ip);
    trusted_proxies().contains(&ip)
}

/// Resolve the client IP for `req`, honouring forwarded metadata only from a trusted peer.
///
/// Returns `None` only when the request has no transport peer.
pub(crate) fn client_ip(req: &HttpRequest) -> Option<IpAddr> {
    let peer = canonicalize(req.peer_addr()?.ip());
    let trusted = trusted_proxies();

    if !trusted.contains(&peer) {
        if req.headers().contains_key(FORWARDED_FOR) {
            // Not a warning: behind an unconfigured proxy this is every request. The
            // equivalent misconfiguration is surfaced loudly on the QR-login path by
            // `api::sessions::get_gateway_url`.
            tracing::debug!(
                peer = %peer,
                "Ignoring {FORWARDED_FOR} from an untrusted peer; keying on the socket address. \
                 Set {TRUSTED_PROXY_ENV} if this peer is a reverse proxy."
            );
        }
        return Some(peer);
    }

    let Some(chain) = req
        .headers()
        .get(FORWARDED_FOR)
        .and_then(|value| value.to_str().ok())
    else {
        return Some(peer);
    };

    let hops: Vec<IpAddr> = chain
        .split(',')
        .map(str::trim)
        .filter_map(|hop| hop.parse::<IpAddr>().ok())
        .map(canonicalize)
        .collect();

    // Walk right to left, skipping hops that are themselves trusted proxies: the first
    // element a trusted proxy did not vouch for is the client. Anything the caller prepended
    // sits to the left of what the proxy appended, so it can never be reached first.
    hops.iter()
        .rev()
        .find(|hop| !trusted.contains(hop))
        .copied()
        // Every hop is a trusted proxy, so the furthest-upstream claim is as far as the
        // trusted chain reaches. An empty/unparseable chain falls back to the socket peer.
        .or_else(|| hops.first().copied())
        .or(Some(peer))
}

/// Client IP rendered as a rate-limit / audit key.
///
/// The value is a canonicalized [`IpAddr`], so one host cannot occupy two buckets by
/// arriving under two spellings.
pub(crate) fn client_ip_key(req: &HttpRequest) -> String {
    client_ip(req).map_or_else(|| UNKNOWN_CLIENT.to_string(), |ip| ip.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    /// `TRUSTED_PROXY_IPS` is process-global; these tests read the default (unset) policy
    /// only, so they never mutate it and cannot race the rest of the suite.
    fn request(peer: &str, forwarded_for: Option<&str>) -> HttpRequest {
        let mut req = TestRequest::default().peer_addr(peer.parse().expect("valid socket addr"));
        if let Some(value) = forwarded_for {
            req = req.insert_header((FORWARDED_FOR, value));
        }
        req.to_http_request()
    }

    #[test]
    fn untrusted_peer_keeps_its_socket_address() {
        let req = request("198.51.100.42:443", Some("203.0.113.7"));
        assert_eq!(client_ip_key(&req), "198.51.100.42");
    }

    #[test]
    fn untrusted_peer_cannot_hide_behind_an_appended_chain() {
        let req = request("198.51.100.42:443", Some("203.0.113.7, 198.51.100.42"));
        assert_eq!(client_ip_key(&req), "198.51.100.42");
    }

    #[test]
    fn trusted_peer_asserts_the_client() {
        let req = request("127.0.0.1:443", Some("203.0.113.7"));
        assert_eq!(client_ip_key(&req), "203.0.113.7");
    }

    #[test]
    fn trusted_peer_ignores_a_prepended_claim() {
        // nginx `$proxy_add_x_forwarded_for` appends the address it saw; the caller supplied
        // everything to its left.
        let req = request("127.0.0.1:443", Some("203.0.113.7, 198.51.100.42"));
        assert_eq!(client_ip_key(&req), "198.51.100.42");
    }

    #[test]
    fn trusted_hops_are_skipped_during_the_walk() {
        // Two loopback proxies in front; the client is upstream of both.
        let req = request("127.0.0.1:443", Some("198.51.100.42, 127.0.0.1, ::1"));
        assert_eq!(client_ip_key(&req), "198.51.100.42");
    }

    #[test]
    fn chain_of_only_trusted_hops_yields_the_furthest_upstream() {
        let req = request("127.0.0.1:443", Some("127.0.0.1, ::1"));
        assert_eq!(client_ip_key(&req), "127.0.0.1");
    }

    #[test]
    fn unparseable_chain_falls_back_to_the_peer() {
        let req = request("127.0.0.1:443", Some("not-an-ip"));
        assert_eq!(client_ip_key(&req), "127.0.0.1");
    }

    #[test]
    fn absent_header_uses_the_peer() {
        let req = request("198.51.100.42:443", None);
        assert_eq!(client_ip_key(&req), "198.51.100.42");
    }

    #[test]
    fn mapped_peer_matches_the_dotted_default_and_does_not_split_keys() {
        // A dual-stack listener reports loopback as `::ffff:127.0.0.1`. It must still be
        // recognised as the trusted default, and must key identically to `127.0.0.1`.
        let mapped = request("[::ffff:127.0.0.1]:443", Some("203.0.113.7"));
        assert_eq!(client_ip_key(&mapped), "203.0.113.7");

        let mapped_client = request("127.0.0.1:443", Some("::ffff:203.0.113.7"));
        assert_eq!(client_ip_key(&mapped_client), "203.0.113.7");
    }

    #[test]
    fn no_transport_peer_yields_unknown() {
        let req = TestRequest::default()
            .insert_header((FORWARDED_FOR, "203.0.113.7"))
            .to_http_request();
        assert_eq!(client_ip_key(&req), UNKNOWN_CLIENT);
    }
}
