use mcptools_core::images::{error_hint, redact_error_body};

#[test]
fn redacts_full_credentials_without_changing_the_message() {
    for secret in [
        "sk-proj-testCredential123456789",
        "eyJ.testBearerToken.signature",
        "token+/with=punctuation",
    ] {
        let body = format!("Incorrect API key provided: {secret}. Retry with {secret}.");
        assert_eq!(
            redact_error_body(&body, secret),
            "Incorrect API key provided: [REDACTED]. Retry with [REDACTED]."
        );
    }
}

#[test]
fn redacts_partial_credential_echoes() {
    let secret = "bearerPrefix123456789bearerSuffix";
    for echo in [
        "bearerPrefix****bearerSuffix",
        "bearerPrefix...bearerSuffix",
        "bearerPrefix…bearerSuffix",
        "bearerPrefix****",
        "****bearerSuffix",
    ] {
        assert_eq!(
            redact_error_body(&format!("Invalid token: {echo}. Try again."), secret),
            "Invalid token: [REDACTED]. Try again."
        );
    }
    assert_eq!(
        redact_error_body(
            "Invalid eyJ.payload...signature.",
            "eyJ.payload.full.signature"
        ),
        "Invalid [REDACTED]."
    );
    assert_eq!(
        redact_error_body("Invalid token+/...ation.", "token+/with=punctuation"),
        "Invalid [REDACTED]."
    );
}

#[test]
fn redacts_other_openai_key_shapes() {
    let body = "Keys: sk-test12345678, sk-proj-other_123-ABC; sk-svcacct-another123. sk-proj-test****tail! sk-abc...xyz?";
    assert_eq!(
        redact_error_body(body, "unrelatedBearerToken"),
        "Keys: [REDACTED], [REDACTED]; [REDACTED]. [REDACTED]! [REDACTED]?"
    );
}

#[test]
fn short_values_and_unrelated_text_stay_intact() {
    let body = "a test failed at auto: invalid sk-test; task-manager mask-pattern; prefix...suffix, **bold**, 日本語";
    for secret in ["", "a", "test", "auto", "invalid", "sk-test", "日本語"] {
        assert_eq!(redact_error_body(body, secret), body);
    }
    assert_eq!(redact_error_body(body, "unrelatedLongToken"), body);
    assert_eq!(redact_error_body("ab***gh", "abcdefgh"), "ab***gh");
    assert_eq!(redact_error_body("sk-unknown123", ""), "[REDACTED]");
    assert_eq!(redact_error_body("12345678", "12345678"), "[REDACTED]");
    assert_eq!(
        redact_error_body("日本語日本語日本", "日本語日本語日本"),
        "[REDACTED]"
    );
}

#[test]
fn overlapping_key_shapes_and_credentials_are_fully_masked() {
    assert_eq!(
        redact_error_body(
            "Invalid bearer-sk-embedded123-token.end",
            "bearer-sk-embedded123-token.end"
        ),
        "Invalid [REDACTED]"
    );
    assert_eq!(
        redact_error_body("sk-secret123suffix", "sk-secret123"),
        "[REDACTED]"
    );
}

#[test]
fn error_hints_keep_status_and_upstream_message() {
    for (status, body) in [
        (401, "Incorrect API key provided: [REDACTED]"),
        (400, "moderation_blocked: revise the input"),
        (403, "organization_verification: verify your organization"),
        (429, "Rate limit exceeded"),
    ] {
        let hint = error_hint(status, body);
        assert!(hint.contains(&status.to_string()), "{hint}");
        assert!(hint.contains(body), "{hint}");
    }
}
