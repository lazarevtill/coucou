// The only way a plugin reaches the network.
//
// A client is built per plugin and knows nothing but the plugin's allowed hosts:
// each one is resolved once and pinned to those addresses, so a DNS answer that
// changes later cannot point the plugin somewhere else. Private and local
// addresses are refused unless the plugin declares it talks to the local
// network; plain http goes nowhere else. Every redirect is checked against the
// same rules, the system proxy is not used (it would resolve names itself),
// and responses are capped in size and time.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use super::manifest::{host_of, Manifest};

/// Largest response a plugin may receive.
pub const MAX_BODY: usize = 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REDIRECTS: usize = 5;

/// Loopback, private, link-local, shared (CGNAT, e.g. Tailscale), unique-local,
/// unspecified, broadcast and multicast addresses: anything that is not a
/// public internet address.
pub fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || a == 0
                || (a == 100 && (64..128).contains(&b))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_private(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

/// Whether a URL is one this plugin may reach: allowed scheme and host.
pub fn url_allowed(url: &reqwest::Url, hosts: &[String], allow_private: bool) -> bool {
    let scheme_ok = url.scheme() == "https" || (url.scheme() == "http" && allow_private);
    let clean = url.username().is_empty() && url.password().is_none();
    scheme_ok && clean && host_of(url).is_some_and(|h| hosts.iter().any(|x| *x == h))
}

/// A plugin's client, with the addresses its hosts were pinned to.
pub struct Guarded {
    http: reqwest::Client,
    /// host -> every address it resolved to when the client was built.
    pinned: std::collections::HashMap<String, Vec<IpAddr>>,
}

impl std::fmt::Debug for Guarded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Guarded").field("pinned", &self.pinned).finish()
    }
}

/// Plain http only to the local network: every address of the host is private.
fn http_ok(url: &reqwest::Url, pinned: &std::collections::HashMap<String, Vec<IpAddr>>) -> bool {
    if url.scheme() != "http" {
        return true;
    }
    let Some(host) = host_of(url) else { return false };
    match host.parse::<IpAddr>() {
        Ok(ip) => is_private(ip),
        Err(_) => pinned.get(&host).is_some_and(|ips| !ips.is_empty() && ips.iter().all(|ip| is_private(*ip))),
    }
}

/// Builds the plugin's client: resolves and pins every allowed host, refusing
/// local addresses unless the plugin talks to the local network.
pub async fn client(m: &Manifest) -> Result<Guarded, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .no_proxy();
    let mut pinned = std::collections::HashMap::new();
    for host in &m.allowed_hosts {
        let ips: Vec<IpAddr> = match host.parse::<IpAddr>() {
            Ok(ip) => vec![ip],
            Err(_) => {
                let resolved: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 443))
                    .await
                    .map_err(|e| format!("{host} could not be resolved: {e}"))?
                    .collect();
                if resolved.is_empty() {
                    return Err(format!("{host} could not be resolved."));
                }
                builder = builder.resolve_to_addrs(host, &resolved);
                resolved.iter().map(SocketAddr::ip).collect()
            }
        };
        if !m.allow_private_network && ips.iter().any(|ip| is_private(*ip)) {
            return Err(format!("{host} is on the local network; the plugin would have to declare \"allowPrivateNetwork\"."));
        }
        pinned.insert(host.clone(), ips);
    }
    let hosts = m.allowed_hosts.clone();
    let allow_private = m.allow_private_network;
    let redirect_pins = pinned.clone();
    builder = builder.redirect(reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= MAX_REDIRECTS {
            attempt.error("too many redirects")
        } else if url_allowed(attempt.url(), &hosts, allow_private) && http_ok(attempt.url(), &redirect_pins) {
            attempt.follow()
        } else {
            let to = host_of(attempt.url()).unwrap_or_default();
            attempt.error(format!("a redirect to {to} is not allowed"))
        }
    }));
    let http = builder.build().map_err(|e| e.to_string())?;
    Ok(Guarded { http, pinned })
}

fn describe(e: reqwest::Error) -> String {
    if e.is_redirect() {
        let why = std::error::Error::source(&e).map(|s| s.to_string()).unwrap_or_default();
        format!("Stopped at a redirect: {why}")
    } else if e.is_timeout() {
        "The server took too long to answer.".into()
    } else {
        "The server could not be reached.".into()
    }
}

fn checked(g: &Guarded, url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "not a web address".to_string())?;
    let hosts: Vec<String> = g.pinned.keys().cloned().collect();
    if !url_allowed(&parsed, &hosts, true) || !http_ok(&parsed, &g.pinned) {
        return Err(format!("{url} is not one of the plugin's allowed hosts, or not over https."));
    }
    Ok(())
}

async fn body(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(describe)? {
        if out.len() + chunk.len() > MAX_BODY {
            return Err(format!("The response is too large (over {} KB).", MAX_BODY / 1024));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

fn with_headers(mut request: reqwest::RequestBuilder, headers: &[(String, String)]) -> reqwest::RequestBuilder {
    for (k, v) in headers {
        request = request.header(k.as_str(), v.as_str());
    }
    request
}

pub async fn get(g: &Guarded, url: &str, headers: &[(String, String)]) -> Result<(u16, Vec<u8>), String> {
    checked(g, url)?;
    let response = with_headers(g.http.get(url), headers).send().await.map_err(describe)?;
    let status = response.status().as_u16();
    Ok((status, body(response).await?))
}

pub async fn post(
    g: &Guarded,
    url: &str,
    headers: &[(String, String)],
    payload: Vec<u8>,
) -> Result<(u16, Vec<(String, String)>, Vec<u8>), String> {
    checked(g, url)?;
    let response = with_headers(g.http.post(url), headers).body(payload).send().await.map_err(describe)?;
    let status = response.status().as_u16();
    let names = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_ascii_lowercase(), v.to_str().unwrap_or("").to_string()))
        .collect();
    Ok((status, names, body(response).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::manifest;
    use crate::test_http::{serve, Canned};
    use serde_json::json;

    fn run<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    fn plugin(hosts: &[&str], private: bool, url: &str) -> Manifest {
        let v = json!({
            "schema": 1, "id": "t", "name": "T", "version": "1", "kind": "http",
            "allowedHosts": hosts, "allowPrivateNetwork": private,
            "http": { "url": url }
        });
        manifest::parse(&serde_json::to_vec(&v).unwrap()).unwrap()
    }

    #[test]
    fn private_and_local_addresses_are_recognised() {
        for ip in ["127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.1", "169.254.1.1", "100.76.11.32", "0.0.0.0", "255.255.255.255", "224.0.0.1", "::1", "fd00::1", "fe80::1", "::", "::ffff:192.168.1.1"] {
            assert!(is_private(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "140.82.112.3", "100.128.0.1", "172.32.0.1", "2606:4700::1111"] {
            assert!(!is_private(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn only_allowed_hosts_and_schemes_pass() {
        let hosts = vec!["api.example.com".to_string(), "192.168.1.10".to_string()];
        let ok = |u: &str, private: bool| url_allowed(&reqwest::Url::parse(u).unwrap(), &hosts, private);
        assert!(ok("https://api.example.com/v1/x", false));
        assert!(ok("https://API.example.com/v1/x", false), "host names compare without case");
        assert!(!ok("https://evil.example.com/", false));
        assert!(!ok("http://api.example.com/", false), "plain http needs the local-network flag");
        assert!(ok("http://192.168.1.10:8123/", true));
        assert!(!ok("ftp://api.example.com/", true));
        assert!(!ok("https://user@api.example.com/", false));
    }

    #[test]
    fn a_name_that_resolves_to_a_local_address_is_refused_unless_allowed() {
        let m = plugin(&["localhost"], false, "https://localhost/x");
        let err = run(client(&m)).unwrap_err();
        assert!(err.contains("localhost") && err.contains("local network"), "{err}");
        assert!(run(client(&plugin(&["localhost"], true, "http://localhost/x"))).is_ok());
    }

    #[test]
    fn a_get_reaches_an_allowed_local_server_with_its_headers() {
        let (origin, seen) = serve(vec![Canned::json(200, r#"{"ok":true}"#)]);
        let m = plugin(&["127.0.0.1"], true, &format!("{origin}/status"));
        let http = run(client(&m)).unwrap();
        let (status, body) = run(get(&http, &format!("{origin}/status"), &[("Authorization".into(), "Bearer t0k".into())])).unwrap();
        assert_eq!((status, body.as_slice()), (200, br#"{"ok":true}"#.as_slice()));
        let req = seen.lock().unwrap()[0].to_lowercase();
        assert!(req.starts_with("get /status "));
        assert!(req.contains("authorization: bearer t0k"));
    }

    #[test]
    fn a_redirect_off_the_allowlist_is_not_followed() {
        let (origin, seen) = serve(vec![Canned {
            status: 302,
            headers: vec![("Location", "http://127.0.0.2:9/elsewhere".into())],
            body: vec![],
        }]);
        let m = plugin(&["127.0.0.1"], true, &format!("{origin}/a"));
        let http = run(client(&m)).unwrap();
        let err = run(get(&http, &format!("{origin}/a"), &[])).unwrap_err();
        assert!(err.contains("redirect"), "{err}");
        assert_eq!(seen.lock().unwrap().len(), 1, "nothing was sent to the other host");
    }

    #[test]
    fn a_redirect_within_the_allowlist_is_followed() {
        let (origin, _) = serve(vec![
            Canned { status: 302, headers: vec![("Location", "/b".into())], body: vec![] },
            Canned::json(200, "{}"),
        ]);
        let m = plugin(&["127.0.0.1"], true, &format!("{origin}/a"));
        let http = run(client(&m)).unwrap();
        assert_eq!(run(get(&http, &format!("{origin}/a"), &[])).unwrap().0, 200);
    }

    #[test]
    fn an_oversized_response_is_cut_off() {
        let (origin, _) = serve(vec![Canned { status: 200, headers: vec![], body: vec![b'x'; MAX_BODY + 10] }]);
        let m = plugin(&["127.0.0.1"], true, &format!("{origin}/big"));
        let http = run(client(&m)).unwrap();
        let err = run(get(&http, &format!("{origin}/big"), &[])).unwrap_err();
        assert!(err.contains("too large"), "{err}");
    }

    #[test]
    fn a_post_returns_status_headers_and_body() {
        let (origin, seen) = serve(vec![Canned {
            status: 200,
            headers: vec![("Content-Type", "application/json".into()), ("Mcp-Session-Id", "abc".into())],
            body: br#"{"jsonrpc":"2.0","id":1,"result":{}}"#.to_vec(),
        }]);
        let m = plugin(&["127.0.0.1"], true, &format!("{origin}/mcp"));
        let http = run(client(&m)).unwrap();
        let (status, headers, body) = run(post(&http, &format!("{origin}/mcp"), &[("Mcp-Method".into(), "tools/list".into())], b"{}".to_vec())).unwrap();
        assert_eq!(status, 200);
        assert!(headers.iter().any(|(k, v)| k == "mcp-session-id" && v == "abc"), "{headers:?}");
        assert!(body.starts_with(b"{\"jsonrpc\""));
        assert!(seen.lock().unwrap()[0].to_lowercase().contains("mcp-method: tools/list"));
    }
}
