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

pub fn should_shutdown(now: u64, started_at: u64, grace: u64, windows: &[ShareWindow]) -> bool {
    now.saturating_sub(started_at) >= grace
        && !windows.iter().any(|window| is_open(Some(window), now))
}

pub fn idle_since(now: u64, previous: Option<u64>, windows: &[ShareWindow]) -> Option<u64> {
    if windows.iter().any(|window| is_open(Some(window), now)) {
        None
    } else {
        Some(
            previous.unwrap_or(now).max(
                windows
                    .iter()
                    .map(|window| window.expires_at_unix_secs)
                    .max()
                    .unwrap_or(0),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_shutdown_requires_grace_and_no_open_windows() {
        let open = ShareWindow {
            expires_at_unix_secs: 300,
        };
        assert!(!should_shutdown(100, 100, 120, &[]));
        assert!(!should_shutdown(220, 100, 120, &[open]));
        assert!(should_shutdown(221, 100, 120, &[]));
    }

    #[test]
    fn grace_restarts_after_an_open_window_closes() {
        let open = ShareWindow {
            expires_at_unix_secs: 1000,
        };
        assert_eq!(idle_since(500, Some(100), &[open]), None);
        assert_eq!(idle_since(501, None, &[]), Some(501));
        assert!(!should_shutdown(620, 501, 120, &[]));
        assert!(should_shutdown(621, 501, 120, &[]));
    }

    #[test]
    fn expiry_between_checks_restarts_idle_grace() {
        let expired = ShareWindow {
            expires_at_unix_secs: 120,
        };
        assert_eq!(idle_since(120, Some(0), &[expired]), Some(120));
        assert!(!should_shutdown(239, 120, 120, &[]));
        assert!(should_shutdown(240, 120, 120, &[]));
    }

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
