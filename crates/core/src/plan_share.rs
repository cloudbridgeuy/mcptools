use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShareWindow {
    pub expires_at_unix_secs: u64,
}

pub fn is_open(window: Option<&ShareWindow>, now: u64) -> bool {
    window.is_some_and(|window| now < window.expires_at_unix_secs)
}

pub fn clamp_ttl(seconds: Option<u64>) -> u64 {
    seconds.unwrap_or(1800).clamp(60, 86400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_window_is_open_only_before_expiry() {
        let window = ShareWindow {
            expires_at_unix_secs: 100,
        };
        assert!(!is_open(None, 0));
        assert!(is_open(Some(&window), 99));
        assert!(!is_open(Some(&window), 100));
        assert!(!is_open(Some(&window), 101));
        assert!(!is_open(
            Some(&ShareWindow {
                expires_at_unix_secs: 0
            }),
            0
        ));
        assert!(!is_open(Some(&window), u64::MAX));
    }

    #[test]
    fn share_ttl_defaults_and_clamps_at_both_bounds() {
        assert_eq!(clamp_ttl(None), 1800);
        for value in [0, 1, 59, 60] {
            assert_eq!(clamp_ttl(Some(value)), 60);
        }
        for value in [61, 1800, 86399, 86400] {
            assert_eq!(clamp_ttl(Some(value)), value);
        }
        assert_eq!(clamp_ttl(Some(86401)), 86400);
        assert_eq!(clamp_ttl(Some(u64::MAX)), 86400);
    }
}
