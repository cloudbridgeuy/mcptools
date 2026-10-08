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
    }
}
