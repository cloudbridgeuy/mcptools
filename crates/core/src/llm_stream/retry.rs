use super::ContractError;

pub fn retry_after_deadline(raw: &str, now_ms: i64) -> Option<i64> {
    let raw = raw.trim();
    if !raw.is_empty() && raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Some(seconds_deadline(
            raw.parse::<u64>().unwrap_or(u64::MAX),
            now_ms,
        ));
    }
    chrono::DateTime::parse_from_rfc2822(raw)
        .ok()
        .map(|date| date.timestamp_millis().max(now_ms))
        .or_else(|| {
            ["%A, %d-%b-%y %H:%M:%S GMT", "%a %b %e %H:%M:%S %Y"]
                .iter()
                .find_map(|format| chrono::NaiveDateTime::parse_from_str(raw, format).ok())
                .map(|date| date.and_utc().timestamp_millis().max(now_ms))
        })
}

pub fn seconds_deadline(seconds: u64, now_ms: i64) -> i64 {
    now_ms.saturating_add(i64::try_from(seconds.saturating_mul(1000)).unwrap_or(i64::MAX))
}

impl ContractError {
    pub fn retry_error(&self) -> &Self {
        match self {
            Self::WithRetryMetadata { error, .. } => error.retry_error(),
            error => error,
        }
    }

    pub fn server_deadline(&self, now_ms: i64) -> Option<i64> {
        match self {
            Self::WithRetryMetadata {
                error,
                retry_after_raw,
                retry_after_seconds,
                ..
            } => {
                let raw = retry_after_raw
                    .as_deref()
                    .and_then(|raw| retry_after_deadline(raw, now_ms));
                let seconds = retry_after_seconds.map(|seconds| seconds_deadline(seconds, now_ms));
                [raw, seconds, error.server_deadline(now_ms)]
                    .into_iter()
                    .flatten()
                    .max()
            }
            Self::RateLimited {
                retry_after_seconds,
            } => retry_after_seconds.map(|seconds| seconds_deadline(seconds, now_ms)),
            _ => None,
        }
    }

    pub fn retry_forbidden(&self) -> bool {
        match self {
            Self::WithRetryMetadata {
                error, retryable, ..
            } => *retryable == Some(false) || error.retry_forbidden(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_dates_and_invalid_headers() {
        assert_eq!(retry_after_deadline("120", 100), Some(120100));
        assert_eq!(
            retry_after_deadline("Thu, 01 Jan 1970 00:02:00 GMT", 100),
            Some(120000)
        );
        for raw in ["", "-1", "+1", "1.5", "bad"] {
            assert_eq!(retry_after_deadline(raw, 100), None);
        }
        assert_eq!(
            retry_after_deadline("Thu, 01 Jan 1970 00:00:00 GMT", 100),
            Some(100)
        );
        assert_eq!(
            retry_after_deadline("18446744073709551615", 100),
            Some(i64::MAX)
        );
        assert_eq!(
            retry_after_deadline("18446744073709551616", 100),
            Some(i64::MAX)
        );
    }

    #[test]
    fn metadata_is_redacted_and_conservative_when_deadlines_disagree() {
        let error = ContractError::WithRetryMetadata {
            error: Box::new(ContractError::RateLimited {
                retry_after_seconds: Some(120),
            }),
            retryable: Some(false),
            retry_after_raw: Some("Thu, 01 Jan 1970 00:01:00 GMT".into()),
            retry_after_seconds: Some(120),
        };
        assert!(error.retry_forbidden());
        assert_eq!(error.server_deadline(100), Some(120100));
        let error = ContractError::WithRetryMetadata {
            error: Box::new(ContractError::Connection("secret".into())),
            retryable: None,
            retry_after_raw: Some("secret".into()),
            retry_after_seconds: None,
        }
        .redacted(&["secret".into()]);
        assert_eq!(
            error.retry_error(),
            &ContractError::Connection("[redacted]".into())
        );
        assert!(
            matches!(error, ContractError::WithRetryMetadata { retry_after_raw: Some(ref raw), .. } if raw == "[redacted]")
        );
    }
}
