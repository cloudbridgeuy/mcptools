use reqwest::header::{HeaderMap, RETRY_AFTER};

pub const DEFAULT_RETRY_AFTER_MS: u64 = 1000;
pub const MAX_READ_ATTEMPTS: u32 = 3;

pub fn is_mutation(query: &str) -> bool {
    let mut rest = query.trim_start();
    while let Some(stripped) = rest.strip_prefix('#') {
        match stripped.find(['\n', '\r']) {
            Some(i) => rest = stripped[i..].trim_start(),
            None => return false,
        }
    }
    match rest.strip_prefix("mutation") {
        None => false,
        Some(tail) => match tail.chars().next() {
            None => true,
            Some(c) => !(c.is_alphanumeric() || c == '_'),
        },
    }
}

pub fn first_error_code(body: &serde_json::Value) -> String {
    body.get("errors")
        .and_then(|errors| errors.as_array())
        .and_then(|items| items.first())
        .and_then(|item| item.get("extensions"))
        .and_then(|extensions| extensions.get("code"))
        .and_then(|code| code.as_str())
        .unwrap_or_default()
        .to_string()
}

pub fn retry_after_ms(headers: &HeaderMap) -> Option<u64> {
    retry_after_ms_at(headers, current_epoch_ms())
}

pub fn retry_after_ms_at(headers: &HeaderMap, now_ms: u64) -> Option<u64> {
    if let Some(wait) = retry_after_header(headers) {
        return Some(wait);
    }
    ["x-ratelimit-requests-reset", "x-ratelimit-complexity-reset"]
        .iter()
        .filter_map(|name| reset_wait(headers, name, now_ms))
        .min()
}

pub fn classify_retry(status: u16, code: &str, headers: &HeaderMap) -> Option<u64> {
    let flat: String = code.chars().filter(|c| *c != '_' && *c != '-').collect();
    let limited = status == 429
        || flat.eq_ignore_ascii_case("ratelimited")
        || (status == 400 && flat.to_uppercase().contains("RATELIMIT"));
    match limited {
        true => Some(retry_after_ms(headers).unwrap_or(DEFAULT_RETRY_AFTER_MS)),
        false => None,
    }
}

pub fn backoff_ms(failed_attempt: u32, hint_ms: u64) -> u64 {
    let growth = 250u64.saturating_mul(1u64 << failed_attempt.min(4));
    growth.max(hint_ms)
}

fn current_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

fn retry_after_header(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|text| text.trim().parse::<u64>().ok())
        .map(|seconds| seconds.saturating_mul(1000))
}

fn reset_wait(headers: &HeaderMap, name: &str, now_ms: u64) -> Option<u64> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|text| text.trim().parse::<u64>().ok())
        .map(|reset_ms| reset_ms.saturating_sub(now_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        map
    }

    #[test]
    fn detects_plain_mutation() {
        assert!(is_mutation("mutation { createIssue(input: {}) { id } }"));
    }

    #[test]
    fn detects_mutation_without_space() {
        assert!(is_mutation("mutation{ createIssue { id } }"));
    }

    #[test]
    fn detects_mutation_with_leading_trivia() {
        assert!(is_mutation(
            "  # fetch first\n  mutation CreateIssue($i: ID!) { x }"
        ));
    }

    #[test]
    fn rejects_queries_and_shorthand() {
        assert!(!is_mutation("query { viewer { id } }"));
        assert!(!is_mutation("{ viewer { id } }"));
        assert!(!is_mutation(""));
    }

    #[test]
    fn rejects_longer_keyword_prefix() {
        assert!(!is_mutation("mutations { x }"));
    }

    #[test]
    fn rejects_comment_only_document() {
        assert!(!is_mutation("# just a comment"));
    }

    #[test]
    fn extracts_first_error_code() {
        let body = serde_json::json!({"errors":[{"message":"Slow down","extensions":{"code":"RATELIMITED"}}]});
        assert_eq!(first_error_code(&body), "RATELIMITED");
    }

    #[test]
    fn returns_empty_code_when_absent() {
        assert_eq!(
            first_error_code(&serde_json::json!({"data":{"viewer":{"id":"u1"}}})),
            ""
        );
        assert_eq!(first_error_code(&serde_json::json!({"errors":[]})), "");
        assert_eq!(
            first_error_code(&serde_json::json!({"errors":[{"message":"No code"}]})),
            ""
        );
        assert_eq!(first_error_code(&serde_json::json!({"errors":"oops"})), "");
    }

    #[test]
    fn prefers_retry_after_over_reset_headers() {
        let map = headers(&[
            ("retry-after", "2"),
            ("x-ratelimit-requests-reset", "9999999999999"),
        ]);
        assert_eq!(retry_after_ms_at(&map, 0), Some(2000));
    }

    #[test]
    fn converts_retry_after_seconds_to_millis() {
        let map = headers(&[("retry-after", "0")]);
        assert_eq!(retry_after_ms_at(&map, 1000), Some(0));
    }

    #[test]
    fn takes_earlier_of_both_reset_headers() {
        let map = headers(&[
            ("x-ratelimit-requests-reset", "12000"),
            ("x-ratelimit-complexity-reset", "8000"),
        ]);
        assert_eq!(retry_after_ms_at(&map, 5000), Some(3000));
    }

    #[test]
    fn uses_single_reset_header_when_only_one_present() {
        let map = headers(&[("x-ratelimit-complexity-reset", "9000")]);
        assert_eq!(retry_after_ms_at(&map, 4000), Some(5000));
    }

    #[test]
    fn treats_elapsed_reset_as_no_wait() {
        let map = headers(&[("x-ratelimit-requests-reset", "1000")]);
        assert_eq!(retry_after_ms_at(&map, 5000), Some(0));
    }

    #[test]
    fn ignores_unparsable_headers() {
        let map = headers(&[
            ("retry-after", "soon"),
            ("x-ratelimit-requests-reset", "not-a-number"),
        ]);
        assert_eq!(retry_after_ms_at(&map, 0), None);
    }

    #[test]
    fn returns_none_without_rate_headers() {
        assert_eq!(retry_after_ms_at(&HeaderMap::new(), 0), None);
    }

    #[test]
    fn flags_http_429_without_code() {
        assert_eq!(
            classify_retry(429, "", &HeaderMap::new()),
            Some(DEFAULT_RETRY_AFTER_MS)
        );
    }

    #[test]
    fn flags_ratelimited_code_on_any_status() {
        let map = headers(&[("retry-after", "3")]);
        assert_eq!(classify_retry(200, "RATELIMITED", &map), Some(3000));
        assert_eq!(classify_retry(200, "ratelimited", &map), Some(3000));
    }

    #[test]
    fn flags_rate_variant_code_on_http_400() {
        assert_eq!(
            classify_retry(400, "RATE_LIMIT_EXCEEDED", &HeaderMap::new()),
            Some(DEFAULT_RETRY_AFTER_MS)
        );
    }

    #[test]
    fn ignores_validation_failure_on_http_400() {
        assert_eq!(
            classify_retry(400, "GRAPHQL_VALIDATION_FAILED", &HeaderMap::new()),
            None
        );
        assert_eq!(classify_retry(400, "", &HeaderMap::new()), None);
    }

    #[test]
    fn ignores_success_and_auth_failures() {
        assert_eq!(classify_retry(200, "", &HeaderMap::new()), None);
        assert_eq!(classify_retry(401, "", &HeaderMap::new()), None);
        assert_eq!(classify_retry(500, "", &HeaderMap::new()), None);
    }

    #[test]
    fn honors_headers_through_classify() {
        let map = headers(&[("retry-after", "4")]);
        assert_eq!(classify_retry(429, "", &map), Some(4000));
        let elapsed = headers(&[("x-ratelimit-requests-reset", "20000")]);
        assert_eq!(classify_retry(429, "", &elapsed), Some(0));
    }

    #[test]
    fn grows_backoff_per_failed_attempt() {
        assert_eq!(backoff_ms(1, 0), 500);
        assert_eq!(backoff_ms(2, 0), 1000);
        assert!(backoff_ms(3, 0) > backoff_ms(2, 0));
    }

    #[test]
    fn honors_hint_above_growth() {
        assert_eq!(backoff_ms(1, 5000), 5000);
        assert_eq!(backoff_ms(1, 500), 500);
    }
}
