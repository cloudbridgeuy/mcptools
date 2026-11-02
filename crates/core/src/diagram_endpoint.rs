use std::net::{Ipv4Addr, SocketAddr};

pub fn tailscale_ip(output: &str) -> Option<Ipv4Addr> {
    output
        .lines()
        .filter_map(|line| line.trim().parse::<Ipv4Addr>().ok())
        .find(|ip| {
            let octets = ip.octets();
            octets[0] == 100 && (64..=127).contains(&octets[1])
        })
}

pub fn endpoint(host: Option<&str>, port: Option<&str>, record: Option<&str>) -> (String, String) {
    let address = record
        .and_then(|value| value.trim().parse::<SocketAddr>().ok())
        .filter(|address| {
            crate::diagram_serve::check_bind_host(address.ip()).is_ok() && address.port() != 0
        });
    (
        host.map(str::to_owned)
            .or_else(|| address.map(|addr| addr.ip().to_string()))
            .unwrap_or_else(|| "127.0.0.1".into()),
        port.map(str::to_owned)
            .or_else(|| address.map(|addr| addr.port().to_string()))
            .unwrap_or_else(|| "5199".into()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tailscale_range_and_invalid_values() {
        assert_eq!(
            tailscale_ip("junk\n100.63.1.2\n100.64.0.1\n100.127.255.255\n"),
            Some(Ipv4Addr::new(100, 64, 0, 1))
        );
        assert_eq!(tailscale_ip("100.128.0.1\n127.0.0.1\n"), None);
    }

    #[test]
    fn endpoint_precedence_and_ipv6() {
        let record = Some("[::1]:4321");
        assert_eq!(endpoint(None, None, record), ("::1".into(), "4321".into()));
        assert_eq!(
            endpoint(Some("example.com"), None, record),
            ("example.com".into(), "4321".into())
        );
        assert_eq!(
            endpoint(None, Some("9999"), record),
            ("::1".into(), "9999".into())
        );
        assert_eq!(
            endpoint(Some("x"), Some("3"), record),
            ("x".into(), "3".into())
        );
        assert_eq!(
            endpoint(None, None, Some("bad")),
            ("127.0.0.1".into(), "5199".into())
        );
    }

    #[test]
    fn endpoint_rejects_wildcards_and_zero_ports() {
        for record in [
            "0.0.0.0:5199",
            "[::]:5199",
            "[::ffff:0.0.0.0]:5199",
            "100.80.20.1:0",
        ] {
            assert_eq!(
                endpoint(None, None, Some(record)),
                ("127.0.0.1".into(), "5199".into())
            );
        }
    }
}
