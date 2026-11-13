pub use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Jira,
    Linear,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    NoPr,
    Draft,
    CiFailing,
    CiPending,
    AwaitingReview,
    ChangesRequested,
    UnresolvedThreads,
    Approved,
    Merged,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlagCode {
    TicketNoPr,
    PrNoTicket,
    MergedTicketOpen,
    TicketDonePrOpen,
    TicketReviewPrDraft,
    CiFail,
    CiFailInheritedFromStackBase,
    UnresolvedThreads,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct PrKey {
    pub repo: String,
    pub number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TicketInput {
    pub source: Source,
    pub key: String,
    pub url: String,
    pub title: String,
    pub status: String,
    pub closed: bool,
    pub updated: String,
    pub pr_refs: Vec<PrKey>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct CiCounts {
    pub pass: u64,
    pub pending: u64,
    pub fail: u64,
    pub skipped: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GithubReview {
    pub state: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GithubReviews {
    #[serde(rename = "latestByReviewer", default)]
    pub latest_by_reviewer: Vec<GithubReview>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GithubThreads {
    pub unresolved: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GithubFailure {
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GithubCi {
    pub summary: CiCounts,
    #[serde(default)]
    pub failing: Vec<GithubFailure>,
    #[serde(rename = "inheritedFromStackParent", default)]
    pub inherited_from_stack_parent: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GithubBase {
    #[serde(rename = "stackParent")]
    pub stack_parent: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubPr {
    pub number: u64,
    pub title: String,
    pub url: Option<String>,
    pub state: String,
    #[serde(rename = "isDraft")]
    pub is_draft: bool,
    #[serde(rename = "mergedAt", default)]
    pub merged_at: Option<String>,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: Option<String>,
    #[serde(rename = "ticketKeys", default)]
    pub ticket_keys: Vec<String>,
    #[serde(default)]
    pub reviews: GithubReviews,
    #[serde(default)]
    pub threads: GithubThreads,
    #[serde(default)]
    pub ci: GithubCi,
    #[serde(default)]
    pub base: GithubBase,
}

#[derive(Debug, Clone)]
pub struct PrInput {
    pub repo: String,
    pub record: GithubPr,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCounts {
    pub approved: u64,
    pub changes_requested: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PrSummary {
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub url: Option<String>,
    pub state: PrState,
    pub is_draft: bool,
    pub stage: Option<Stage>,
    pub ci: CiCounts,
    pub reviews: ReviewCounts,
    pub threads_unresolved: u64,
    pub inherited_ci_failures: Vec<String>,
    pub stack_parent: Option<u64>,
    pub updated_at: Option<String>,
    pub flags: Vec<FlagCode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub source: Source,
    pub key: String,
    pub url: String,
    pub title: String,
    pub status: String,
    pub stage: Stage,
    pub prs: Vec<PrSummary>,
    pub flags: Vec<FlagCode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Flag {
    pub code: FlagCode,
    pub ticket: Option<String>,
    pub pr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceError {
    pub source: String,
    pub scope: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub kind: ChangeKind,
    pub source: Source,
    pub key: String,
    pub stage_from: Option<Stage>,
    pub stage_to: Option<Stage>,
    pub flags_added: Vec<FlagCode>,
    pub flags_removed: Vec<FlagCode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SnapshotItem {
    pub source: Source,
    pub key: String,
    pub stage: Stage,
    pub flags: Vec<FlagCode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Snapshot {
    pub version: u32,
    pub items: Vec<SnapshotItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub items: Vec<Item>,
    pub orphan_prs: Vec<PrSummary>,
    pub flags: Vec<Flag>,
    pub source_errors: Vec<SourceError>,
    pub markdown: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<Change>")]
    pub changes: Option<Vec<Change>>,
}

pub struct ReportInput {
    pub tickets: Vec<TicketInput>,
    pub prs: Vec<PrInput>,
    pub source_errors: Vec<SourceError>,
    pub as_of: NaiveDate,
    pub stale_days: i64,
    pub snapshot: Option<Snapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotName(String);

const MAX_SNAPSHOT_NAME: usize = 64;

impl SnapshotName {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let valid = !raw.is_empty()
            && raw.len() <= MAX_SNAPSHOT_NAME
            && raw
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'));
        if valid {
            Ok(Self(raw.to_owned()))
        } else {
            Err(format!(
                "snapshot name must be 1-{MAX_SNAPSHOT_NAME} characters of A-Z, a-z, 0-9, _ or -"
            ))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn file_name(&self) -> String {
        format!("{}.json", self.0)
    }
}

pub fn parse_as_of(raw: &str) -> Result<NaiveDate, String> {
    raw.get(..10)
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
        .ok_or_else(|| format!("asOf must start with YYYY-MM-DD: {raw}"))
}

pub fn as_of_from_unix_secs(secs: i64) -> Result<NaiveDate, String> {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|time| time.date_naive())
        .ok_or_else(|| format!("invalid clock time: {secs}"))
}

fn pr_ref_from_url(url: &str) -> Option<PrKey> {
    let pattern = regex::Regex::new(r"github\.com/([^/\s]+)/([^/\s]+)/pull/([0-9]+)")
        .expect("valid pull request url pattern");
    let captures = pattern.captures(url)?;
    Some(PrKey {
        repo: format!("{}/{}", &captures[1], &captures[2]),
        number: captures[3].parse().ok()?,
    })
}

pub fn linear_ticket(record: &crate::linear::StatusRecord) -> TicketInput {
    TicketInput {
        source: Source::Linear,
        key: record.identifier.clone(),
        url: record.url.clone(),
        title: record.title.clone(),
        status: record.state.name.clone(),
        closed: matches!(record.normalized_status.as_str(), "done" | "canceled"),
        updated: record.updated_at.clone(),
        pr_refs: record
            .pr_refs
            .iter()
            .map(|r| PrKey {
                repo: format!("{}/{}", r.owner, r.repo),
                number: r.number,
            })
            .collect(),
    }
}

pub fn jira_ticket(record: &crate::atlassian::jira::status::StatusRecord) -> TicketInput {
    TicketInput {
        source: Source::Jira,
        key: record.key.clone(),
        url: record.url.clone(),
        title: record.summary.clone(),
        status: record.status.name.clone(),
        closed: record.status.category == "done"
            || matches!(record.normalized_status.as_str(), "done" | "canceled"),
        updated: record.updated.clone(),
        pr_refs: record
            .pr_refs
            .iter()
            .filter_map(|r| pr_ref_from_url(&r.url))
            .collect(),
    }
}

fn is_merged(record: &GithubPr) -> bool {
    record.merged_at.as_deref().is_some_and(|at| !at.is_empty())
}

fn pr_state(record: &GithubPr) -> PrState {
    if is_merged(record) {
        PrState::Merged
    } else if record.state.eq_ignore_ascii_case("open") {
        PrState::Open
    } else {
        PrState::Closed
    }
}

const BLOCKING_ORDER: [Stage; 7] = [
    Stage::Draft,
    Stage::CiFailing,
    Stage::ChangesRequested,
    Stage::CiPending,
    Stage::UnresolvedThreads,
    Stage::AwaitingReview,
    Stage::Approved,
];

fn blocking_rank(stage: Stage) -> usize {
    BLOCKING_ORDER
        .iter()
        .position(|s| *s == stage)
        .unwrap_or(BLOCKING_ORDER.len())
}

fn review_counts(record: &GithubPr) -> ReviewCounts {
    let count = |state: &str| {
        record
            .reviews
            .latest_by_reviewer
            .iter()
            .filter(|r| r.state.eq_ignore_ascii_case(state))
            .count() as u64
    };
    ReviewCounts {
        approved: count("approved"),
        changes_requested: count("changes_requested"),
    }
}

fn open_stage(record: &GithubPr, reviews: &ReviewCounts) -> Stage {
    [
        (record.is_draft, Stage::Draft),
        (record.ci.summary.fail > 0, Stage::CiFailing),
        (reviews.changes_requested > 0, Stage::ChangesRequested),
        (record.ci.summary.pending > 0, Stage::CiPending),
        (record.threads.unresolved > 0, Stage::UnresolvedThreads),
        (reviews.approved > 0, Stage::Approved),
    ]
    .into_iter()
    .find_map(|(applies, stage)| applies.then_some(stage))
    .unwrap_or(Stage::AwaitingReview)
}

fn days_since(timestamp: &str, as_of: NaiveDate) -> Option<i64> {
    parse_as_of(timestamp)
        .ok()
        .map(|date| as_of.signed_duration_since(date).num_days())
}

fn pr_flags(record: &GithubPr, state: PrState) -> Vec<FlagCode> {
    if state != PrState::Open {
        return Vec::new();
    }
    let inherited: BTreeSet<&str> = record
        .ci
        .inherited_from_stack_parent
        .iter()
        .map(String::as_str)
        .collect();
    let own_failure = record.ci.summary.fail > 0
        && (record.ci.failing.is_empty()
            || record
                .ci
                .failing
                .iter()
                .any(|f| !inherited.contains(f.name.as_str())));
    [
        (own_failure, FlagCode::CiFail),
        (
            !inherited.is_empty() && record.ci.summary.fail > 0,
            FlagCode::CiFailInheritedFromStackBase,
        ),
        (record.threads.unresolved > 0, FlagCode::UnresolvedThreads),
    ]
    .into_iter()
    .filter_map(|(applies, code)| applies.then_some(code))
    .collect()
}

fn summarize(pr: &PrInput) -> PrSummary {
    let record = &pr.record;
    let state = pr_state(record);
    let reviews = review_counts(record);
    let stage = match state {
        PrState::Merged => Some(Stage::Merged),
        PrState::Open => Some(open_stage(record, &reviews)),
        PrState::Closed => None,
    };
    PrSummary {
        repo: pr.repo.clone(),
        number: record.number,
        title: record.title.clone(),
        url: record.url.clone(),
        state,
        is_draft: record.is_draft,
        stage,
        ci: record.ci.summary.clone(),
        reviews,
        threads_unresolved: record.threads.unresolved,
        inherited_ci_failures: record.ci.inherited_from_stack_parent.clone(),
        stack_parent: record.base.stack_parent,
        updated_at: record.updated_at.clone(),
        flags: pr_flags(record, state),
    }
}

fn pr_label(pr: &PrSummary) -> String {
    format!("{}#{}", pr.repo, pr.number)
}

fn item_stage(prs: &[PrSummary]) -> Stage {
    prs.iter()
        .filter(|p| p.state == PrState::Open)
        .filter_map(|p| p.stage)
        .min_by_key(|stage| blocking_rank(*stage))
        .or_else(|| {
            prs.iter()
                .any(|p| p.state == PrState::Merged)
                .then_some(Stage::Merged)
        })
        .unwrap_or(Stage::NoPr)
}

fn is_stale(
    last_activity: impl Iterator<Item = String>,
    as_of: NaiveDate,
    stale_days: i64,
) -> bool {
    last_activity
        .filter_map(|ts| days_since(&ts, as_of))
        .min()
        .is_some_and(|days| days > stale_days)
}

fn ticket_flags(
    ticket: &TicketInput,
    stage: Stage,
    prs: &[PrSummary],
    sources: &BTreeMap<(String, u64), &PrInput>,
    as_of: NaiveDate,
    stale_days: i64,
) -> Vec<FlagCode> {
    let open: Vec<&PrSummary> = prs.iter().filter(|p| p.state == PrState::Open).collect();
    let activity = std::iter::once(ticket.updated.clone()).chain(
        open.iter()
            .filter_map(|p| sources.get(&(p.repo.clone(), p.number)))
            .filter_map(|p| p.record.updated_at.clone()),
    );
    let mut flags: Vec<FlagCode> = [
        (stage == Stage::NoPr && !ticket.closed, FlagCode::TicketNoPr),
        (
            stage == Stage::Merged && !ticket.closed,
            FlagCode::MergedTicketOpen,
        ),
        (
            ticket.closed && !open.is_empty(),
            FlagCode::TicketDonePrOpen,
        ),
        (
            ticket.status.to_ascii_lowercase().contains("review")
                && open.iter().any(|p| p.is_draft),
            FlagCode::TicketReviewPrDraft,
        ),
        (
            !ticket.closed && is_stale(activity, as_of, stale_days),
            FlagCode::Stale,
        ),
    ]
    .into_iter()
    .filter_map(|(applies, code)| applies.then_some(code))
    .collect();
    flags.sort();
    flags
}

fn matches_ticket(ticket: &TicketInput, pr: &PrInput) -> bool {
    ticket
        .pr_refs
        .iter()
        .any(|r| r.repo == pr.repo && r.number == pr.record.number)
        || pr
            .record
            .ticket_keys
            .iter()
            .any(|k| k.eq_ignore_ascii_case(&ticket.key))
}

pub fn snapshot_of(report: &Report) -> Snapshot {
    Snapshot {
        version: 1,
        items: report
            .items
            .iter()
            .map(|item| SnapshotItem {
                source: item.source,
                key: item.key.clone(),
                stage: item.stage,
                flags: item.flags.clone(),
            })
            .collect(),
    }
}

fn diff(old: &Snapshot, new: &Snapshot) -> Vec<Change> {
    let before: BTreeMap<(Source, &str), &SnapshotItem> = old
        .items
        .iter()
        .map(|i| ((i.source, i.key.as_str()), i))
        .collect();
    let after: BTreeMap<(Source, &str), &SnapshotItem> = new
        .items
        .iter()
        .map(|i| ((i.source, i.key.as_str()), i))
        .collect();
    let mut changes = Vec::new();
    for (key, item) in &after {
        match before.get(key) {
            None => changes.push(Change {
                kind: ChangeKind::Added,
                source: item.source,
                key: item.key.clone(),
                stage_from: None,
                stage_to: Some(item.stage),
                flags_added: item.flags.clone(),
                flags_removed: Vec::new(),
            }),
            Some(prev) if prev.stage != item.stage || prev.flags != item.flags => {
                changes.push(Change {
                    kind: ChangeKind::Changed,
                    source: item.source,
                    key: item.key.clone(),
                    stage_from: Some(prev.stage),
                    stage_to: Some(item.stage),
                    flags_added: item
                        .flags
                        .iter()
                        .filter(|f| !prev.flags.contains(f))
                        .copied()
                        .collect(),
                    flags_removed: prev
                        .flags
                        .iter()
                        .filter(|f| !item.flags.contains(f))
                        .copied()
                        .collect(),
                })
            }
            Some(_) => {}
        }
    }
    for (key, item) in &before {
        if !after.contains_key(key) {
            changes.push(Change {
                kind: ChangeKind::Removed,
                source: item.source,
                key: item.key.clone(),
                stage_from: Some(item.stage),
                stage_to: None,
                flags_added: Vec::new(),
                flags_removed: item.flags.clone(),
            });
        }
    }
    let rank = |kind: ChangeKind| match kind {
        ChangeKind::Added => 0,
        ChangeKind::Changed => 1,
        ChangeKind::Removed => 2,
    };
    changes.sort_by(|a, b| (rank(a.kind), a.source, &a.key).cmp(&(rank(b.kind), b.source, &b.key)));
    changes
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn stage_name(stage: Stage) -> String {
    serde_json::to_value(stage)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn code_name(code: FlagCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn render(report: &Report) -> String {
    let mut out = String::from(
        "| Ticket | Status | PR(s) | Stage | CI | Reviews | Threads |\n|---|---|---|---|---|---|---|\n",
    );
    for item in &report.items {
        let open: Vec<&PrSummary> = item
            .prs
            .iter()
            .filter(|p| p.state == PrState::Open)
            .collect();
        let prs = if item.prs.is_empty() {
            "-".to_owned()
        } else {
            item.prs.iter().map(pr_label).collect::<Vec<_>>().join(", ")
        };
        let (ci, reviews, threads) = if open.is_empty() {
            ("-".to_owned(), "-".to_owned(), "-".to_owned())
        } else {
            (
                format!(
                    "pass:{} pend:{} fail:{}",
                    open.iter().map(|p| p.ci.pass).sum::<u64>(),
                    open.iter().map(|p| p.ci.pending).sum::<u64>(),
                    open.iter().map(|p| p.ci.fail).sum::<u64>()
                ),
                format!(
                    "approved:{} changes:{}",
                    open.iter().map(|p| p.reviews.approved).sum::<u64>(),
                    open.iter()
                        .map(|p| p.reviews.changes_requested)
                        .sum::<u64>()
                ),
                open.iter()
                    .map(|p| p.threads_unresolved)
                    .sum::<u64>()
                    .to_string(),
            )
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            cell(&item.key),
            cell(&item.status),
            cell(&prs),
            stage_name(item.stage),
            ci,
            reviews,
            threads
        ));
    }
    out.push_str("\n## Flags\n");
    if report.flags.is_empty() {
        out.push_str("None\n");
    }
    for flag in &report.flags {
        let subject = flag.ticket.as_deref().or(flag.pr.as_deref()).unwrap_or("");
        out.push_str(&format!("- {} {}\n", code_name(flag.code), subject));
    }
    if !report.orphan_prs.is_empty() {
        out.push_str("\n## PRs without ticket\n");
        for pr in &report.orphan_prs {
            out.push_str(&format!("- {} {}\n", pr_label(pr), cell(&pr.title)));
        }
    }
    if !report.source_errors.is_empty() {
        out.push_str("\n## Source errors\n");
        for error in &report.source_errors {
            let scope = error
                .scope
                .as_deref()
                .map(|s| format!(" {s}"))
                .unwrap_or_default();
            out.push_str(&format!(
                "- {}{}: {}\n",
                error.source,
                scope,
                cell(&error.message)
            ));
        }
    }
    if let Some(changes) = &report.changes {
        out.push_str("\n## Changes since snapshot\n");
        if changes.is_empty() {
            out.push_str("None\n");
        }
        for change in changes {
            let kind = serde_json::to_value(change.kind)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            let stage = |s: Option<Stage>| s.map(stage_name).unwrap_or_else(|| "-".into());
            out.push_str(&format!(
                "- {} {}: {} -> {}\n",
                kind,
                change.key,
                stage(change.stage_from),
                stage(change.stage_to)
            ));
        }
    }
    out
}

pub fn build_report(input: ReportInput) -> Report {
    let mut tickets: BTreeMap<(Source, String), TicketInput> = BTreeMap::new();
    for ticket in input.tickets {
        tickets
            .entry((ticket.source, ticket.key.to_ascii_uppercase()))
            .or_insert(ticket);
    }
    let mut prs: BTreeMap<(String, u64), PrInput> = BTreeMap::new();
    for pr in input.prs {
        prs.entry((pr.repo.clone(), pr.record.number)).or_insert(pr);
    }
    let by_key: BTreeMap<(String, u64), &PrInput> =
        prs.iter().map(|(k, v)| (k.clone(), v)).collect();
    let mut matched: BTreeSet<(String, u64)> = BTreeSet::new();
    let mut items = Vec::new();
    let mut flags = Vec::new();
    for ticket in tickets.values() {
        let linked: Vec<&PrInput> = prs
            .values()
            .filter(|pr| matches_ticket(ticket, pr))
            .collect();
        matched.extend(linked.iter().map(|pr| (pr.repo.clone(), pr.record.number)));
        let summaries: Vec<PrSummary> = linked.iter().map(|pr| summarize(pr)).collect();
        let stage = item_stage(&summaries);
        let ticket_level = ticket_flags(
            ticket,
            stage,
            &summaries,
            &by_key,
            input.as_of,
            input.stale_days,
        );
        flags.extend(ticket_level.iter().map(|code| Flag {
            code: *code,
            ticket: Some(ticket.key.clone()),
            pr: None,
        }));
        let mut all: Vec<FlagCode> = ticket_level
            .into_iter()
            .chain(summaries.iter().flat_map(|p| p.flags.iter().copied()))
            .collect();
        all.sort();
        all.dedup();
        items.push(Item {
            source: ticket.source,
            key: ticket.key.clone(),
            url: ticket.url.clone(),
            title: ticket.title.clone(),
            status: ticket.status.clone(),
            stage,
            prs: summaries,
            flags: all,
        });
    }
    let mut seen_pr_flags: BTreeSet<(String, FlagCode)> = BTreeSet::new();
    let mut pr_flag_entries = Vec::new();
    for item in &items {
        for pr in &item.prs {
            for code in &pr.flags {
                if seen_pr_flags.insert((pr_label(pr), *code)) {
                    pr_flag_entries.push(Flag {
                        code: *code,
                        ticket: None,
                        pr: Some(pr_label(pr)),
                    });
                }
            }
        }
    }
    let orphan_prs: Vec<PrSummary> = prs
        .iter()
        .filter(|(key, pr)| !matched.contains(*key) && pr_state(&pr.record) == PrState::Open)
        .map(|(_, pr)| {
            let mut summary = summarize(pr);
            summary.flags.push(FlagCode::PrNoTicket);
            if is_stale(
                pr.record.updated_at.clone().into_iter(),
                input.as_of,
                input.stale_days,
            ) {
                summary.flags.push(FlagCode::Stale);
            }
            summary.flags.sort();
            summary
        })
        .collect();
    for pr in &orphan_prs {
        for code in &pr.flags {
            pr_flag_entries.push(Flag {
                code: *code,
                ticket: None,
                pr: Some(pr_label(pr)),
            });
        }
    }
    flags.extend(pr_flag_entries);
    flags.sort_by(|a, b| (&a.ticket, &a.pr, a.code).cmp(&(&b.ticket, &b.pr, b.code)));
    let mut source_errors = input.source_errors;
    source_errors
        .sort_by(|a, b| (&a.source, &a.scope, &a.message).cmp(&(&b.source, &b.scope, &b.message)));
    let mut report = Report {
        items,
        orphan_prs,
        flags,
        source_errors,
        markdown: String::new(),
        changes: None,
    };
    report.changes = input
        .snapshot
        .as_ref()
        .map(|old| diff(old, &snapshot_of(&report)));
    report.markdown = render(&report);
    report
}

pub fn author_search_args(author: &str, page: u32) -> Result<serde_json::Value, String> {
    let valid = !author.is_empty()
        && author
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-');
    if !valid {
        return Err(format!("invalid GitHub author: {author:?}"));
    }
    Ok(serde_json::json!({
        "query": format!("is:pr is:open author:{author}"),
        "perPage": 100,
        "page": page,
    }))
}

fn repo_of_search_item(item: &serde_json::Value) -> Option<String> {
    let segments = |url: &str, marker: &str, skip: usize| -> Option<String> {
        let rest = url.split_once(marker)?.1;
        let mut parts = rest.split('/').skip(skip);
        let (owner, name) = (parts.next()?, parts.next()?);
        (!owner.is_empty() && !name.is_empty()).then(|| format!("{owner}/{name}"))
    };
    item["repository_url"]
        .as_str()
        .and_then(|url| segments(url, "/repos/", 0))
        .or_else(|| {
            item["html_url"]
                .as_str()
                .and_then(|url| segments(url, "://", 1))
        })
}

pub fn pr_keys_from_search(items: &[serde_json::Value]) -> Result<Vec<PrKey>, String> {
    items
        .iter()
        .map(|item| {
            let number = item["number"]
                .as_u64()
                .filter(|number| *number > 0)
                .ok_or("Invalid GitHub search result number")?;
            let repo =
                repo_of_search_item(item).ok_or("Invalid GitHub search result repository")?;
            Ok(PrKey { repo, number })
        })
        .collect::<Result<BTreeSet<_>, String>>()
        .map(|keys| keys.into_iter().collect())
}

pub fn view_requests(keys: &[PrKey]) -> Vec<(String, serde_json::Value)> {
    let mut grouped: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
    for key in keys {
        grouped.entry(&key.repo).or_default().push(key.number);
    }
    grouped
        .into_iter()
        .map(|(repo, numbers)| {
            (
                repo.to_owned(),
                serde_json::json!({"repo": repo, "numbers": numbers}),
            )
        })
        .collect()
}

#[cfg(test)]
mod author_search_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn search_args_query_every_repo_for_the_authors_open_prs() {
        let args = author_search_args("guzmonne", 2).unwrap();
        assert_eq!(args["query"], "is:pr is:open author:guzmonne");
        assert_eq!(args["perPage"], 100);
        assert_eq!(args["page"], 2);
        assert!(args.get("owner").is_none());
        assert!(args.get("repo").is_none());
    }

    #[test]
    fn search_args_reject_authors_that_could_alter_the_query() {
        assert!(author_search_args("a b", 1).is_err());
        assert!(author_search_args("a repo:x/y", 1).is_err());
        assert!(author_search_args("", 1).is_err());
    }

    #[test]
    fn search_results_become_sorted_unique_keys_across_repos() {
        let items = [
            json!({"number":9,"repository_url":"https://api.github.com/repos/acme/zeta"}),
            json!({"number":3,"repository_url":"https://api.github.com/repos/acme/alpha"}),
            json!({"number":3,"repository_url":"https://api.github.com/repos/acme/alpha"}),
            json!({"number":5,"html_url":"https://github.com/other/beta/pull/5"}),
        ];
        let keys = pr_keys_from_search(&items).unwrap();
        let shown: Vec<(&str, u64)> = keys.iter().map(|k| (k.repo.as_str(), k.number)).collect();
        assert_eq!(
            shown,
            [("acme/alpha", 3), ("acme/zeta", 9), ("other/beta", 5)]
        );
    }

    #[test]
    fn search_results_without_a_repo_or_number_are_errors() {
        assert!(pr_keys_from_search(&[json!({"number":1})]).is_err());
        assert!(pr_keys_from_search(&[
            json!({"repository_url":"https://api.github.com/repos/a/b"})
        ])
        .is_err());
    }

    #[test]
    fn view_requests_group_numbers_per_repo() {
        let key = |repo: &str, number| PrKey {
            repo: repo.into(),
            number,
        };
        let requests = view_requests(&[key("a/x", 1), key("a/x", 2), key("b/y", 7)]);
        assert_eq!(
            requests,
            [
                ("a/x".to_owned(), json!({"repo":"a/x","numbers":[1,2]})),
                ("b/y".to_owned(), json!({"repo":"b/y","numbers":[7]})),
            ]
        );
    }
}
