use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::PathBuf;

use mcptools_core::atlas::{
    content_hash, parse_description, FileDescription, ParseDescriptionError,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub enum SourceInput {
    File(Vec<u8>),
    Directory(BTreeMap<String, Vec<u8>>),
}

#[derive(Debug, Clone, Serialize)]
pub struct EnrichmentInput {
    pub source: SourceInput,
    pub template: Vec<u8>,
    pub system: String,
    pub prompt: String,
    pub variables: BTreeMap<String, String>,
    pub context: Option<Vec<u8>>,
    pub provider: String,
    pub model: String,
    pub model_revision: Option<String>,
    pub settings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputIdentity(pub(super) String);

impl EnrichmentInput {
    pub fn identity(&self) -> Result<InputIdentity, serde_json::Error> {
        let bytes = serde_json::to_vec(&("atlas-enrichment-v1", self))?;
        Ok(InputIdentity(content_hash(&bytes).hex().to_string()))
    }
}

impl InputIdentity {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
}

impl EntryKind {
    pub(super) fn table(self) -> &'static str {
        match self {
            Self::File => "files",
            Self::Directory => "directories",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrichmentTarget {
    pub kind: EntryKind,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrichmentState {
    Missing,
    Current(InputIdentity),
    Stale,
    Failed(String),
    Deferred(String),
    InFlight {
        identity: InputIdentity,
        lease_until: i64,
    },
}

#[derive(Debug)]
pub struct EnrichmentClaim {
    pub(super) target: EnrichmentTarget,
    pub(super) identity: InputIdentity,
    pub(super) token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsuccessfulWork {
    Failed(String),
    Deferred(String),
}

#[derive(Debug)]
pub struct EnrichmentCandidate {
    pub target: EnrichmentTarget,
    pub identity: InputIdentity,
    pub state: EnrichmentState,
    pub context_tokens: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStatus {
    Reusable,
    Missing,
    Stale,
    Failed,
    Deferred,
    InFlight,
}

impl PlanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reusable => "reusable",
            Self::Missing => "missing",
            Self::Stale => "stale",
            Self::Failed => "failed",
            Self::Deferred => "deferred",
            Self::InFlight => "in-flight",
        }
    }

    fn schedulable(self) -> bool {
        !matches!(self, Self::Reusable | Self::InFlight)
    }
}

#[derive(Debug)]
pub struct EnrichmentPlanEntry {
    pub target: EnrichmentTarget,
    pub status: PlanStatus,
    pub context_tokens: usize,
    pub scheduled: bool,
}

#[derive(Debug)]
pub struct EnrichmentPlan {
    pub entries: Vec<EnrichmentPlanEntry>,
}

impl EnrichmentPlan {
    pub fn expected_calls(&self) -> usize {
        self.entries.iter().filter(|entry| entry.scheduled).count()
    }

    pub fn pending(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.status.schedulable())
            .count()
    }

    pub fn count(&self, status: PlanStatus) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.status == status)
            .count()
    }

    pub fn scheduled_context_tokens(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.scheduled)
            .map(|entry| entry.context_tokens)
            .sum()
    }
}

pub fn plan_enrichment(
    mut candidates: Vec<EnrichmentCandidate>,
    refresh: bool,
    limit: Option<usize>,
) -> EnrichmentPlan {
    candidates.sort_by(|left, right| target_order(&left.target, &right.target));
    let mut remaining = limit.unwrap_or(usize::MAX);
    let entries = candidates
        .into_iter()
        .map(|candidate| {
            let status = classify(&candidate, refresh);
            let scheduled = status.schedulable() && remaining > 0;
            if scheduled {
                remaining -= 1;
            }
            EnrichmentPlanEntry {
                target: candidate.target,
                status,
                context_tokens: candidate.context_tokens,
                scheduled,
            }
        })
        .collect();
    EnrichmentPlan { entries }
}

fn classify(candidate: &EnrichmentCandidate, refresh: bool) -> PlanStatus {
    match &candidate.state {
        EnrichmentState::Current(identity) if !refresh && identity == &candidate.identity => {
            PlanStatus::Reusable
        }
        EnrichmentState::Current(_) | EnrichmentState::Stale => PlanStatus::Stale,
        EnrichmentState::Missing => PlanStatus::Missing,
        EnrichmentState::Failed(_) => PlanStatus::Failed,
        EnrichmentState::Deferred(_) => PlanStatus::Deferred,
        EnrichmentState::InFlight { identity, .. }
            if refresh || identity != &candidate.identity =>
        {
            PlanStatus::Stale
        }
        EnrichmentState::InFlight { .. } => PlanStatus::InFlight,
    }
}

fn target_order(left: &EnrichmentTarget, right: &EnrichmentTarget) -> Ordering {
    match (left.kind, right.kind) {
        (EntryKind::File, EntryKind::Directory) => Ordering::Less,
        (EntryKind::Directory, EntryKind::File) => Ordering::Greater,
        (EntryKind::File, EntryKind::File) => left.path.cmp(&right.path),
        (EntryKind::Directory, EntryKind::Directory) => right
            .path
            .components()
            .count()
            .cmp(&left.path.components().count())
            .then_with(|| left.path.cmp(&right.path)),
    }
}

pub struct ValidatedDescription(pub(super) FileDescription);

#[derive(Debug, thiserror::Error)]
pub enum DescriptionError {
    #[error(transparent)]
    Parse(#[from] ParseDescriptionError),
    #[error("failure marker is not a description")]
    FailureMarker,
}

impl ValidatedDescription {
    pub fn parse(response: &str) -> Result<Self, DescriptionError> {
        let mut lines = response.trim().lines();
        if !lines
            .next()
            .is_some_and(|line| line.trim_start().starts_with("SHORT:"))
        {
            return Err(ParseDescriptionError::MissingShort.into());
        }
        if !lines
            .find(|line| !line.trim().is_empty())
            .is_some_and(|line| line.trim_start().starts_with("LONG:"))
        {
            return Err(ParseDescriptionError::MissingLong.into());
        }
        let description = parse_description(response)?;
        if description.short == "[description failed]" {
            return Err(DescriptionError::FailureMarker);
        }
        Ok(Self(description))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn input(bytes: &[u8]) -> EnrichmentInput {
        EnrichmentInput {
            source: SourceInput::File(bytes.to_vec()),
            template: b"template".to_vec(),
            system: "system".into(),
            prompt: "prompt".into(),
            variables: BTreeMap::new(),
            context: None,
            provider: "provider".into(),
            model: "model".into(),
            model_revision: None,
            settings: BTreeMap::new(),
        }
    }

    #[test]
    fn identity_covers_every_input_and_canonicalizes_maps() {
        let original = input(b"source");
        let identity = original.identity().unwrap();
        let changes: &[fn(&mut EnrichmentInput)] = &[
            |i| i.source = SourceInput::File(b"changed".to_vec()),
            |i| {
                i.source = SourceInput::Directory(BTreeMap::from([(
                    "child".into(),
                    b"used summary".to_vec(),
                )]))
            },
            |i| i.template.push(0),
            |i| i.system.push('x'),
            |i| i.prompt.push('x'),
            |i| {
                i.variables.insert("language".into(), "rust".into());
            },
            |i| i.context = Some(Vec::new()),
            |i| i.provider.push('x'),
            |i| i.model.push('x'),
            |i| i.model_revision = Some("revision".into()),
            |i| {
                i.settings.insert("temperature".into(), "0".into());
            },
        ];
        for change in changes {
            let mut changed = original.clone();
            change(&mut changed);
            assert_ne!(identity, changed.identity().unwrap());
        }
        let mut a = original.clone();
        a.variables.insert("a".into(), "1".into());
        a.variables.insert("b".into(), "2".into());
        let mut b = original;
        b.variables.insert("b".into(), "2".into());
        b.variables.insert("a".into(), "1".into());
        assert_eq!(a.identity().unwrap(), b.identity().unwrap());
        a.source = SourceInput::Directory(BTreeMap::from([("child".into(), b"old".to_vec())]));
        b.source = SourceInput::Directory(BTreeMap::from([("child".into(), b"new".to_vec())]));
        assert_ne!(a.identity().unwrap(), b.identity().unwrap());
        assert!(ValidatedDescription::parse("SHORT: [description failed]\nLONG: failure").is_err());
        assert!(ValidatedDescription::parse("SHORT: valid\nLONG:").is_err());
        assert!(ValidatedDescription::parse("unstructured prose").is_err());
    }

    #[test]
    fn planner_reuses_exact_hits_and_limits_pending_work_in_stable_order() {
        let current = input(b"current").identity().unwrap();
        let changed = input(b"changed").identity().unwrap();
        let candidate = |kind, path: &str, identity: InputIdentity, state| EnrichmentCandidate {
            target: EnrichmentTarget {
                kind,
                path: path.into(),
            },
            identity,
            state,
            context_tokens: 10,
        };
        let plan = plan_enrichment(
            vec![
                candidate(
                    EntryKind::Directory,
                    "src",
                    changed.clone(),
                    EnrichmentState::Stale,
                ),
                candidate(
                    EntryKind::File,
                    "src/b.rs",
                    changed.clone(),
                    EnrichmentState::Missing,
                ),
                candidate(
                    EntryKind::File,
                    "src/a.rs",
                    current.clone(),
                    EnrichmentState::Current(current),
                ),
                candidate(
                    EntryKind::Directory,
                    "src/nested",
                    changed.clone(),
                    EnrichmentState::Failed("retry".into()),
                ),
                candidate(
                    EntryKind::File,
                    "src/c.rs",
                    changed.clone(),
                    EnrichmentState::Current(input(b"old").identity().unwrap()),
                ),
            ],
            false,
            Some(2),
        );
        let paths: Vec<_> = plan
            .entries
            .iter()
            .map(|entry| entry.target.path.to_string_lossy().to_string())
            .collect();
        assert_eq!(
            paths,
            ["src/a.rs", "src/b.rs", "src/c.rs", "src/nested", "src"]
        );
        assert_eq!(plan.count(PlanStatus::Reusable), 1);
        assert_eq!(plan.count(PlanStatus::Missing), 1);
        assert_eq!(plan.count(PlanStatus::Stale), 2);
        assert_eq!(plan.count(PlanStatus::Failed), 1);
        assert_eq!(plan.expected_calls(), 2);
        assert!(!plan.entries[0].scheduled);
        assert!(plan.entries[1].scheduled);
        assert!(plan.entries[2].scheduled);
        assert!(!plan.entries[3].scheduled);
    }

    #[test]
    fn refresh_turns_reusable_hits_into_work() {
        let identity = input(b"source").identity().unwrap();
        let plan = plan_enrichment(
            vec![EnrichmentCandidate {
                target: EnrichmentTarget {
                    kind: EntryKind::File,
                    path: "src/lib.rs".into(),
                },
                identity: identity.clone(),
                state: EnrichmentState::Current(identity),
                context_tokens: 7,
            }],
            true,
            None,
        );
        assert_eq!(plan.count(PlanStatus::Stale), 1);
        assert_eq!(plan.expected_calls(), 1);
        assert_eq!(plan.scheduled_context_tokens(), 7);
    }

    #[test]
    fn active_claims_remain_visible_without_consuming_the_limit() {
        let identity = input(b"source").identity().unwrap();
        let plan = plan_enrichment(
            vec![
                EnrichmentCandidate {
                    target: EnrichmentTarget {
                        kind: EntryKind::File,
                        path: "a.rs".into(),
                    },
                    identity: identity.clone(),
                    state: EnrichmentState::InFlight {
                        identity: identity.clone(),
                        lease_until: 100,
                    },
                    context_tokens: 3,
                },
                EnrichmentCandidate {
                    target: EnrichmentTarget {
                        kind: EntryKind::File,
                        path: "b.rs".into(),
                    },
                    identity,
                    state: EnrichmentState::Missing,
                    context_tokens: 4,
                },
            ],
            false,
            Some(1),
        );
        assert_eq!(plan.count(PlanStatus::InFlight), 1);
        assert!(!plan.entries[0].scheduled);
        assert!(plan.entries[1].scheduled);
        assert_eq!(plan.expected_calls(), 1);

        let changed = plan_enrichment(
            vec![EnrichmentCandidate {
                target: EnrichmentTarget {
                    kind: EntryKind::File,
                    path: "a.rs".into(),
                },
                identity: input(b"changed").identity().unwrap(),
                state: EnrichmentState::InFlight {
                    identity: input(b"source").identity().unwrap(),
                    lease_until: 100,
                },
                context_tokens: 3,
            }],
            false,
            None,
        );
        assert_eq!(changed.count(PlanStatus::Stale), 1);
        assert!(changed.entries[0].scheduled);
    }
}
