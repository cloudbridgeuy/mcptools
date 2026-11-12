use mcptools_core::work_status::{
    build_report, jira_ticket, linear_ticket, parse_as_of, snapshot_of, FlagCode, GithubPr,
    PrInput, PrState, Report, ReportInput, Snapshot, SnapshotName, SourceError, Stage, TicketInput,
};
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/work_status_input.json")).unwrap()
}

fn tickets(fixture: &Value) -> Vec<TicketInput> {
    let linear: Vec<mcptools_core::linear::StatusRecord> =
        serde_json::from_value(fixture["linear"]["nodes"].clone()).unwrap();
    let jira: Vec<mcptools_core::atlassian::jira::status::StatusRecord> =
        serde_json::from_value(fixture["jira"]["nodes"].clone()).unwrap();
    linear
        .iter()
        .map(linear_ticket)
        .chain(jira.iter().map(jira_ticket))
        .collect()
}

fn prs(fixture: &Value) -> Vec<PrInput> {
    fixture["github"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|repo| {
            repo["pullRequests"]
                .as_array()
                .unwrap()
                .iter()
                .map(|record| PrInput {
                    repo: repo["repo"].as_str().unwrap().to_owned(),
                    record: serde_json::from_value::<GithubPr>(record.clone()).unwrap(),
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn input(fixture: &Value) -> ReportInput {
    ReportInput {
        tickets: tickets(fixture),
        prs: prs(fixture),
        source_errors: Vec::new(),
        as_of: parse_as_of(fixture["asOf"].as_str().unwrap()).unwrap(),
        stale_days: fixture["staleDays"].as_i64().unwrap(),
        snapshot: None,
    }
}

fn report() -> Report {
    build_report(input(&fixture()))
}

#[test]
fn items_sorted_by_source_then_key_with_stages() {
    let report = report();
    let keys: Vec<_> = report.items.iter().map(|i| i.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "VIT-27908",
            "VIT-27910",
            "VIT-27911",
            "GUZ-901",
            "GUZ-902",
            "GUZ-903",
            "GUZ-904"
        ]
    );
    let stages: Vec<_> = report.items.iter().map(|i| i.stage).collect();
    assert_eq!(
        stages,
        [
            Stage::Draft,
            Stage::NoPr,
            Stage::NoPr,
            Stage::UnresolvedThreads,
            Stage::Merged,
            Stage::NoPr,
            Stage::AwaitingReview
        ]
    );
}

#[test]
fn stacked_prs_map_to_one_ticket_and_orphans_are_listed() {
    let report = report();
    let stack = report.items.iter().find(|i| i.key == "VIT-27908").unwrap();
    let numbers: Vec<_> = stack.prs.iter().map(|p| p.number).collect();
    assert_eq!(numbers, [19606, 19676, 19689]);
    let orphans: Vec<_> = report.orphan_prs.iter().map(|p| p.number).collect();
    assert_eq!(orphans, [20002]);
}

#[test]
fn flags_use_fixed_codes_and_deterministic_order() {
    let flags: Vec<_> = report()
        .flags
        .into_iter()
        .map(|f| (f.ticket, f.pr, f.code))
        .collect();
    let pr = |n: u64| Some(format!("acme/vitals#{n}"));
    let ticket = |k: &str| Some(k.to_owned());
    assert_eq!(
        flags,
        [
            (None, pr(19606), FlagCode::CiFail),
            (None, pr(19676), FlagCode::CiFailInheritedFromStackBase),
            (None, pr(19689), FlagCode::CiFailInheritedFromStackBase),
            (None, pr(20001), FlagCode::UnresolvedThreads),
            (None, pr(20002), FlagCode::PrNoTicket),
            (None, pr(20002), FlagCode::Stale),
            (ticket("GUZ-902"), None, FlagCode::MergedTicketOpen),
            (ticket("GUZ-903"), None, FlagCode::TicketNoPr),
            (ticket("GUZ-904"), None, FlagCode::TicketDonePrOpen),
            (ticket("VIT-27908"), None, FlagCode::TicketReviewPrDraft),
            (ticket("VIT-27910"), None, FlagCode::TicketNoPr),
            (ticket("VIT-27910"), None, FlagCode::Stale),
            (ticket("VIT-27911"), None, FlagCode::TicketNoPr),
        ]
    );
}

#[test]
fn markdown_has_table_then_flag_list() {
    let markdown = report().markdown;
    assert!(markdown.starts_with(
        "| Ticket | Status | PR(s) | Stage | CI | Reviews | Threads |\n|---|---|---|---|---|---|---|\n"
    ));
    assert!(markdown.contains("| VIT-27908 | In Review | acme/vitals#19606, acme/vitals#19676, acme/vitals#19689 | draft |"));
    assert!(markdown.contains("\n## Flags\n"));
    assert!(markdown.contains("- CI_FAIL_INHERITED_FROM_STACK_BASE acme/vitals#19676"));
    assert!(markdown.contains("- TICKET_NO_PR VIT-27911"));
}

#[test]
fn output_is_identical_across_runs_and_input_order() {
    let first = serde_json::to_string(&report()).unwrap();
    let mut shuffled = input(&fixture());
    shuffled.tickets.reverse();
    shuffled.prs.reverse();
    assert_eq!(
        serde_json::to_string(&build_report(shuffled)).unwrap(),
        first
    );
    assert!(first.len() > 1000);
}

#[test]
fn failed_source_is_listed_and_report_still_renders() {
    let mut input = input(&fixture());
    input.source_errors.push(SourceError {
        source: "jira".into(),
        scope: Some("VIT".into()),
        message: "401 unauthorized".into(),
    });
    let report = build_report(input);
    assert_eq!(report.source_errors.len(), 1);
    assert_eq!(report.source_errors[0].source, "jira");
    assert!(!report.items.is_empty());
    assert!(report
        .markdown
        .contains("## Source errors\n- jira VIT: 401 unauthorized"));
}

#[test]
fn closed_pr_without_merged_at_is_not_merged() {
    let mut fixture = fixture();
    let github = fixture["github"][0]["pullRequests"].as_array_mut().unwrap();
    let merged = github.iter_mut().find(|p| p["number"] == 19500).unwrap();
    merged["mergedAt"] = Value::Null;
    let report = build_report(input(&fixture));
    let item = report.items.iter().find(|i| i.key == "GUZ-902").unwrap();
    assert_eq!(item.prs[0].state, PrState::Closed);
    assert_eq!(item.stage, Stage::NoPr);
    assert!(!report
        .flags
        .iter()
        .any(|f| f.code == FlagCode::MergedTicketOpen));
}

#[test]
fn pr_record_without_merged_at_field_deserializes_as_not_merged() {
    let record: GithubPr = serde_json::from_value(json!({
        "number": 1, "title": "t", "url": null, "state": "closed", "isDraft": false
    }))
    .unwrap();
    assert_eq!(record.merged_at, None);
    let report = build_report(ReportInput {
        tickets: vec![],
        prs: vec![PrInput {
            repo: "o/r".into(),
            record,
        }],
        source_errors: vec![],
        as_of: parse_as_of("2026-11-10").unwrap(),
        stale_days: 7,
        snapshot: None,
    });
    assert!(report.orphan_prs.is_empty());
}

#[test]
fn snapshot_names_reject_paths_and_odd_characters() {
    for bad in [
        "/etc/passwd",
        "../x",
        "a/b",
        "a\\b",
        "..",
        ".",
        "",
        "a.json",
        "a b",
        "é",
        "a\0b",
        &"x".repeat(65),
    ] {
        assert!(SnapshotName::parse(bad).is_err(), "accepted {bad:?}");
    }
    for good in ["daily", "week-45_a", "A1"] {
        assert_eq!(
            SnapshotName::parse(good).unwrap().file_name(),
            format!("{good}.json")
        );
    }
}

#[test]
fn snapshot_diff_reports_added_removed_and_changed() {
    let base = report();
    let mut old: Snapshot = snapshot_of(&base);
    assert_eq!(old.items.len(), 7);
    old.items.retain(|i| i.key != "GUZ-903");
    old.items.push(mcptools_core::work_status::SnapshotItem {
        source: mcptools_core::work_status::Source::Linear,
        key: "GUZ-1".into(),
        stage: Stage::Draft,
        flags: vec![],
    });
    let guz_901 = old.items.iter_mut().find(|i| i.key == "GUZ-901").unwrap();
    guz_901.stage = Stage::Approved;
    guz_901.flags = vec![FlagCode::Stale];
    let mut input = input(&fixture());
    input.snapshot = Some(old);
    let changes = build_report(input).changes.unwrap();
    let summary: Vec<_> = changes
        .iter()
        .map(|c| (format!("{:?}", c.kind), c.key.as_str()))
        .collect();
    assert_eq!(
        summary,
        [
            ("Added".to_owned(), "GUZ-903"),
            ("Changed".to_owned(), "GUZ-901"),
            ("Removed".to_owned(), "GUZ-1"),
        ]
    );
    let changed = &changes[1];
    assert_eq!(changed.stage_from, Some(Stage::Approved));
    assert_eq!(changed.stage_to, Some(Stage::UnresolvedThreads));
    assert_eq!(changed.flags_added, [FlagCode::UnresolvedThreads]);
    assert_eq!(changed.flags_removed, [FlagCode::Stale]);
}

#[test]
fn report_matches_golden_files() {
    let report = report();
    let expected_json: Value =
        serde_json::from_str(include_str!("fixtures/work_status_expected.json")).unwrap();
    assert_eq!(serde_json::to_value(&report).unwrap(), expected_json);
    assert_eq!(
        report.markdown,
        include_str!("fixtures/work_status_expected.md")
    );
}
