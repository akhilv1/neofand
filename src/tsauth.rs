//! Caller identity from Tailscale.
//!
//! The control endpoints need to know *who* is asking, not merely that someone
//! reached the port. Tailscale already knows: every tailnet address maps to a
//! node and a user, established by WireGuard key exchange rather than by a
//! secret the client presents. Asking `tailscaled` who owns the peer address is
//! therefore stronger than any shared token this daemon could hold, and there is
//! no secret to leak or rotate.
//!
//! ## The spoofing caveat that makes the bind address load-bearing
//!
//! This maps *address → identity*. It is only sound if the address cannot be
//! forged, which means the packet must have arrived through the tunnel. Binding
//! the listener to the `tailscale0` address guarantees that. Binding to
//! `0.0.0.0` does not: a host on the physical LAN could then send a packet with
//! a `100.x` source address and be attributed to whichever tailnet peer owns it.
//! `web.rs` refuses to enable control on a wildcard bind for exactly this
//! reason.
//!
//! ## Protocol
//!
//! `tailscaled` serves a local HTTP API on a Unix socket. `whois` takes an
//! `ip:port` and returns the node and `UserProfile`. Shelling out to
//! `tailscale whois` would work too, but that is a process spawn per request and
//! this is a handful of bytes over a socket.

use std::{
    io::{Read, Write},
    net::IpAddr,
    os::unix::net::UnixStream,
};

const SOCKET_PATHS: &[&str] = &[
    "/var/run/tailscale/tailscaled.sock",
    "/run/tailscale/tailscaled.sock",
];

/// Tailscale's CGNAT range, `100.64.0.0/10`.
///
/// Checked before asking `tailscaled` anything: an address outside it cannot be
/// a tailnet peer, so there is nothing to look up.
fn is_tailnet_addr(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            a == 100 && (64..128).contains(&b)
        }
        // Tailscale's ULA range fd7a:115c:a1e0::/48.
        IpAddr::V6(v6) => {
            let s = v6.segments();
            s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
        }
    }
}

pub struct Identity {
    pub login: String,
    pub node: String,
}

/// Look up who owns `peer`. `None` when the address is not a tailnet address,
/// `tailscaled` is unreachable, or it does not recognise the peer.
pub fn whois(peer: std::net::SocketAddr) -> Option<Identity> {
    if !is_tailnet_addr(peer.ip()) {
        return None;
    }

    let mut stream = SOCKET_PATHS
        .iter()
        .find_map(|path| UnixStream::connect(path).ok())?;

    // The LocalAPI requires this exact Host and rejects other values as a
    // DNS-rebinding guard, so it is not arbitrary.
    let request = format!(
        "GET /localapi/v0/whois?addr={} HTTP/1.1\r\nHost: local-tailscaled.sock\r\n\
         Connection: close\r\n\r\n",
        urlencode(&peer.to_string())
    );
    stream.write_all(request.as_bytes()).ok()?;

    let mut body = String::new();
    stream.read_to_string(&mut body).ok()?;

    if !body.starts_with("HTTP/1.1 200") && !body.starts_with("HTTP/1.0 200") {
        return None;
    }

    // Deliberately a targeted scan rather than a JSON parser: the response is a
    // large nested document and the daemon carries no JSON dependency. Both
    // field names are unique within it.
    Some(Identity {
        login: field(&body, "\"LoginName\":")?,
        node: field(&body, "\"Name\":").unwrap_or_else(|| "unknown".to_owned()),
    })
}

/// Extract the string value following `key` in a JSON document.
fn field(body: &str, key: &str) -> Option<String> {
    let rest = body.split_once(key)?.1;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Is this caller allowed to use the control endpoints?
///
/// Fails closed: an empty allowlist authorises nobody. A config that enables
/// control but forgets to name anyone should reject everything, not everyone.
pub fn authorized(peer: std::net::SocketAddr, allowed: &[String]) -> Result<Identity, String> {
    if allowed.is_empty() {
        return Err("no control_users configured".to_owned());
    }
    let Some(identity) = whois(peer) else {
        return Err(format!("cannot identify caller {peer} via Tailscale"));
    };
    if allowed
        .iter()
        .any(|a| a.eq_ignore_ascii_case(&identity.login))
    {
        Ok(identity)
    } else {
        Err(format!("{} is not permitted", identity.login))
    }
}

#[cfg(test)]
mod tests {
    use super::{field, is_tailnet_addr};

    #[test]
    fn cgnat_range_is_recognised() {
        assert!(is_tailnet_addr("100.99.26.65".parse().unwrap()));
        assert!(is_tailnet_addr("100.64.0.1".parse().unwrap()));
        // 100.127.255.255 is the top of 100.64.0.0/10; .128 is outside it.
        assert!(is_tailnet_addr("100.127.255.255".parse().unwrap()));
        assert!(!is_tailnet_addr("100.128.0.1".parse().unwrap()));
        assert!(!is_tailnet_addr("100.63.255.255".parse().unwrap()));
        assert!(!is_tailnet_addr("192.168.1.10".parse().unwrap()));
        assert!(!is_tailnet_addr("127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn extracts_login_from_whois_body() {
        let body = r#"{"Node":{"Name":"box.ts.net."},"UserProfile":{"LoginName":"a@b.com"}}"#;
        assert_eq!(field(body, "\"LoginName\":").as_deref(), Some("a@b.com"));
        assert_eq!(field(body, "\"Missing\":"), None);
    }
}
