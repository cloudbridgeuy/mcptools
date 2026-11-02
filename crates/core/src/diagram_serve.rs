use std::net::{IpAddr, SocketAddr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route<'a> {
    Page(&'a str),
    Snapshot(&'a str),
    Asset(&'a str),
    Forbidden,
    NotFound,
    MethodNotAllowed,
}

pub fn check_bind_host(host: IpAddr) -> Result<(), String> {
    let wildcard = host.is_unspecified()
        || matches!(host, IpAddr::V6(ip) if ip.to_ipv4_mapped().is_some_and(|ip| ip.is_unspecified()));
    if wildcard {
        Err(format!(
            "refusing wildcard host {host}; bind a specific IP address"
        ))
    } else {
        Ok(())
    }
}

pub fn host_matches(bound: SocketAddr, hosts: &[&str]) -> bool {
    matches!(hosts, [host] if host.parse::<SocketAddr>().ok() == Some(bound))
}

pub fn token_matches(expected: &str, actual: &str) -> bool {
    if expected.len() != 32 || actual.len() != 32 {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in expected.bytes().zip(actual.bytes()) {
        difference |= std::hint::black_box(left) ^ std::hint::black_box(right);
        difference |=
            std::hint::black_box(!(left.is_ascii_digit() || (b'a'..=b'f').contains(&left)) as u8);
    }
    std::hint::black_box(difference) == 0
}

pub fn request_route<'a>(
    bound: SocketAddr,
    hosts: &[&str],
    method: &str,
    path: &'a str,
) -> Route<'a> {
    if !host_matches(bound, hosts) {
        return Route::Forbidden;
    }
    if method != "GET" {
        return Route::MethodNotAllowed;
    }
    if let Some(asset) = path.strip_prefix("/assets/") {
        return Route::Asset(asset);
    }
    let Some(plan) = path.strip_prefix("/d/") else {
        return Route::NotFound;
    };
    match plan.split_once('/') {
        None if !plan.is_empty() => Route::Page(plan),
        Some((token, "plan.tldr")) if !token.is_empty() => Route::Snapshot(token),
        _ => Route::NotFound,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseKind {
    Page,
    Snapshot,
    Asset,
    Forbidden,
    NotFound,
    MethodNotAllowed,
}

pub fn response_kind(route: Route<'_>, found: bool) -> ResponseKind {
    match route {
        Route::Page(_) if found => ResponseKind::Page,
        Route::Snapshot(_) if found => ResponseKind::Snapshot,
        Route::Asset(_) if found => ResponseKind::Asset,
        Route::Forbidden => ResponseKind::Forbidden,
        Route::MethodNotAllowed => ResponseKind::MethodNotAllowed,
        _ => ResponseKind::NotFound,
    }
}

pub fn asset_content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("png") => "image/png",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagram_assets_have_browser_content_types() {
        for (path, expected) in [
            ("/assets/0_merged-hash.svg", "image/svg+xml"),
            ("/assets/font-hash.woff2", "font/woff2"),
            ("/assets/embed-hash.png", "image/png"),
            ("/assets/en-hash.json", "application/json"),
            ("/assets/index-hash.js", "text/javascript; charset=utf-8"),
            ("/assets/index-hash.css", "text/css; charset=utf-8"),
            ("/assets/unknown.bin", "application/octet-stream"),
            ("/assets/missing", "application/octet-stream"),
        ] {
            assert_eq!(asset_content_type(path), expected, "asset {path}");
        }
    }

    #[test]
    fn wildcard_addresses_are_refused() {
        for host in ["0.0.0.0", "::", "::ffff:0.0.0.0"] {
            assert!(check_bind_host(host.parse().unwrap())
                .unwrap_err()
                .contains("refusing"));
        }
        for host in ["127.0.0.1", "100.80.20.1", "::1"] {
            assert!(check_bind_host(host.parse().unwrap()).is_ok());
        }
    }

    #[test]
    fn host_requires_exact_bound_address_and_port() {
        let bound = "127.0.0.1:5199".parse().unwrap();
        assert!(host_matches(bound, &["127.0.0.1:5199"]));
        for hosts in [
            vec![],
            vec!["127.0.0.1"],
            vec!["localhost:5199"],
            vec!["127.0.0.1:80"],
            vec!["127.0.0.1:5199", "127.0.0.1:5199"],
            vec!["attacker.invalid:5199"],
            vec!["127.0.0.1:5199@attacker.invalid"],
        ] {
            assert!(!host_matches(bound, &hosts));
        }
        assert!(host_matches(
            "[::1]:5199".parse().unwrap(),
            &["[0:0:0:0:0:0:0:1]:5199"]
        ));
    }

    #[test]
    fn tokens_require_all_128_bits_and_lowercase_hex() {
        let token = "0123456789abcdef0123456789abcdef";
        assert!(token_matches(token, token));
        for index in 0..32 {
            let mut changed = token.as_bytes().to_vec();
            changed[index] = b'f' - (changed[index] == b'f') as u8;
            assert!(!token_matches(
                token,
                std::str::from_utf8(&changed).unwrap()
            ));
        }
        for invalid in [
            "",
            &token[..31],
            "0123456789abcdef0123456789abcdef0",
            "0123456789ABCDEF0123456789ABCDEF",
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        ] {
            assert!(!token_matches(token, invalid));
            assert!(!token_matches(invalid, invalid));
        }
    }

    #[test]
    fn routes_and_responses_fail_closed() {
        let bound = "127.0.0.1:5199".parse().unwrap();
        let hosts = ["127.0.0.1:5199"];
        for (path, route, response) in [
            ("/d/token", Route::Page("token"), ResponseKind::Page),
            (
                "/d/token/plan.tldr",
                Route::Snapshot("token"),
                ResponseKind::Snapshot,
            ),
            (
                "/assets/app.js",
                Route::Asset("app.js"),
                ResponseKind::Asset,
            ),
        ] {
            assert_eq!(request_route(bound, &hosts, "GET", path), route);
            assert_eq!(response_kind(route, true), response);
            assert_eq!(response_kind(route, false), ResponseKind::NotFound);
        }
        for path in ["/", "/d/", "/d/token/", "/d/token/save", "/d//plan.tldr"] {
            assert_eq!(request_route(bound, &hosts, "GET", path), Route::NotFound);
        }
        assert_eq!(
            request_route(bound, &[], "GET", "/d/token"),
            Route::Forbidden
        );
        assert_eq!(
            request_route(bound, &hosts, "POST", "/d/token"),
            Route::MethodNotAllowed
        );
        assert_eq!(
            response_kind(Route::Forbidden, true),
            ResponseKind::Forbidden
        );
        assert_eq!(
            response_kind(Route::MethodNotAllowed, true),
            ResponseKind::MethodNotAllowed
        );
        assert_eq!(
            asset_content_type("app.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(asset_content_type("app.css"), "text/css; charset=utf-8");
        assert_eq!(asset_content_type("missing"), "application/octet-stream");
    }
}
