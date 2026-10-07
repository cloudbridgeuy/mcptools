use super::types::{Label, Project, Team, WorkflowState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TeamSelector {
    Id(String),
    Key(String),
    Name(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TeamResolution {
    Resolved(Team),
    Ambiguous(Vec<Team>),
    NotFound(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProjectSelector {
    Id(String),
    Name(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProjectResolution {
    Resolved(Project),
    Ambiguous(Vec<Project>),
    NotFound(String),
}

pub fn parse_team_selector(input: &str) -> Option<TeamSelector> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if is_uuid(trimmed) {
        return Some(TeamSelector::Id(trimmed.to_string()));
    }
    if is_key(trimmed) {
        return Some(TeamSelector::Key(trimmed.to_string()));
    }
    Some(TeamSelector::Name(trimmed.to_string()))
}

pub fn match_teams(selector: &TeamSelector, candidates: &[Team]) -> TeamResolution {
    match selector {
        TeamSelector::Id(id) => match candidates.iter().find(|team| team.id == *id) {
            Some(team) => TeamResolution::Resolved(team.clone()),
            None => TeamResolution::NotFound(id.clone()),
        },
        TeamSelector::Key(key) => {
            let hits: Vec<Team> = candidates
                .iter()
                .filter(|team| team.key.eq_ignore_ascii_case(key))
                .cloned()
                .collect();
            match hits.len() {
                1 => TeamResolution::Resolved(hits[0].clone()),
                0 => TeamResolution::NotFound(key.clone()),
                _ => TeamResolution::Ambiguous(hits),
            }
        }
        TeamSelector::Name(name) => {
            let hits: Vec<Team> = candidates
                .iter()
                .filter(|team| team.name.eq_ignore_ascii_case(name))
                .cloned()
                .collect();
            match hits.len() {
                1 => TeamResolution::Resolved(hits[0].clone()),
                0 => TeamResolution::NotFound(name.clone()),
                _ => TeamResolution::Ambiguous(hits),
            }
        }
    }
}

pub fn parse_project_selector(input: &str) -> Option<ProjectSelector> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if is_uuid(trimmed) {
        return Some(ProjectSelector::Id(trimmed.to_string()));
    }
    Some(ProjectSelector::Name(trimmed.to_string()))
}

pub fn match_projects(selector: &ProjectSelector, candidates: &[Project]) -> ProjectResolution {
    match selector {
        ProjectSelector::Id(id) => match candidates.iter().find(|item| item.id == *id) {
            Some(item) => ProjectResolution::Resolved(item.clone()),
            None => ProjectResolution::NotFound(id.clone()),
        },
        ProjectSelector::Name(name) => {
            let hits: Vec<Project> = candidates
                .iter()
                .filter(|item| item.name.eq_ignore_ascii_case(name))
                .cloned()
                .collect();
            match hits.len() {
                1 => ProjectResolution::Resolved(hits[0].clone()),
                0 => ProjectResolution::NotFound(name.clone()),
                _ => ProjectResolution::Ambiguous(hits),
            }
        }
    }
}

pub fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StateSelector {
    Id(String),
    Name(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StateResolution {
    Resolved(WorkflowState),
    NotFound(String),
}

pub fn parse_state_selector(input: &str) -> Option<StateSelector> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if is_uuid(trimmed) {
        return Some(StateSelector::Id(trimmed.to_string()));
    }
    Some(StateSelector::Name(trimmed.to_string()))
}

pub fn match_state(selector: &StateSelector, candidates: &[WorkflowState]) -> StateResolution {
    match selector {
        StateSelector::Id(id) => match candidates.iter().find(|item| item.id == *id) {
            Some(item) => StateResolution::Resolved(item.clone()),
            None => StateResolution::NotFound(id.clone()),
        },
        StateSelector::Name(name) => {
            match candidates
                .iter()
                .find(|item| item.name.eq_ignore_ascii_case(name))
            {
                Some(item) => StateResolution::Resolved(item.clone()),
                None => StateResolution::NotFound(name.clone()),
            }
        }
    }
}

fn is_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 10
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

pub fn team_key_from_identifier(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    let (key, number) = trimmed.rsplit_once('-')?;
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    is_key(key).then_some(key)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LabelSelector {
    Id(String),
    Name(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LabelResolution {
    Resolved(Label),
    NotFound(String),
}

pub fn parse_label_selector(input: &str) -> Option<LabelSelector> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if is_uuid(trimmed) {
        return Some(LabelSelector::Id(trimmed.to_string()));
    }
    Some(LabelSelector::Name(trimmed.to_string()))
}

pub fn match_label(selector: &LabelSelector, candidates: &[Label]) -> LabelResolution {
    match selector {
        LabelSelector::Id(id) => match candidates.iter().find(|item| item.id == *id) {
            Some(item) => LabelResolution::Resolved(item.clone()),
            None => LabelResolution::NotFound(id.clone()),
        },
        LabelSelector::Name(name) => {
            match candidates
                .iter()
                .find(|item| item.name.eq_ignore_ascii_case(name))
            {
                Some(item) => LabelResolution::Resolved(item.clone()),
                None => LabelResolution::NotFound(name.clone()),
            }
        }
    }
}

pub fn format_label_candidates(candidates: &[Label]) -> String {
    candidates
        .iter()
        .map(|item| item.name.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn teams() -> Vec<Team> {
        vec![
            Team {
                id: "t1".to_string(),
                key: "GUZ".to_string(),
                name: "Guzman".to_string(),
            },
            Team {
                id: "t2".to_string(),
                key: "OPS".to_string(),
                name: "Shared".to_string(),
            },
            Team {
                id: "t3".to_string(),
                key: "LAB".to_string(),
                name: "Shared".to_string(),
            },
        ]
    }

    #[test]
    fn parses_uuid_as_id() {
        let id = "e3b567ba-bcd1-42d8-8ae8-129fd11c97ab";
        assert_eq!(
            parse_team_selector(id),
            Some(TeamSelector::Id(id.to_string()))
        );
    }

    #[test]
    fn parses_uppercase_key() {
        assert_eq!(
            parse_team_selector("GUZ"),
            Some(TeamSelector::Key("GUZ".to_string()))
        );
    }

    #[test]
    fn parses_other_text_as_name() {
        assert_eq!(
            parse_team_selector("Guzman"),
            Some(TeamSelector::Name("Guzman".to_string()))
        );
    }

    #[test]
    fn rejects_empty_selector() {
        assert_eq!(parse_team_selector("   "), None);
    }

    #[test]
    fn resolves_team_by_id() {
        let found = match_teams(&TeamSelector::Id("t1".to_string()), &teams());
        assert!(matches!(found, TeamResolution::Resolved(_)));
    }

    #[test]
    fn resolves_team_by_key_case_insensitively() {
        let found = match_teams(&TeamSelector::Key("guz".to_string()), &teams());
        match found {
            TeamResolution::Resolved(team) => assert_eq!(team.id, "t1"),
            other => panic!("expected resolved, got {other:?}"),
        }
    }

    #[test]
    fn reports_ambiguous_team_name() {
        let found = match_teams(&TeamSelector::Name("Shared".to_string()), &teams());
        match found {
            TeamResolution::Ambiguous(hits) => assert_eq!(hits.len(), 2),
            other => panic!("expected ambiguous, got {other:?}"),
        }
    }

    #[test]
    fn reports_missing_team() {
        let found = match_teams(&TeamSelector::Key("NOPE".to_string()), &teams());
        assert!(matches!(found, TeamResolution::NotFound(_)));
    }

    #[test]
    fn resolves_project_by_id_and_name() {
        let items = vec![Project {
            id: "p1".to_string(),
            name: "MCPTools".to_string(),
        }];
        let by_id = match_projects(&ProjectSelector::Id("p1".to_string()), &items);
        assert!(matches!(by_id, ProjectResolution::Resolved(_)));
        let by_name = match_projects(&ProjectSelector::Name("mcptools".to_string()), &items);
        assert!(matches!(by_name, ProjectResolution::Resolved(_)));
        let missing = match_projects(&ProjectSelector::Name("nope".to_string()), &items);
        assert!(matches!(missing, ProjectResolution::NotFound(_)));
    }

    #[test]
    fn rejects_empty_project_selector() {
        assert_eq!(parse_project_selector(""), None);
    }

    #[test]
    fn parses_state_id_and_name() {
        let id = "e3b567ba-bcd1-42d8-8ae8-129fd11c97ab";
        assert_eq!(
            parse_state_selector(id),
            Some(StateSelector::Id(id.to_string()))
        );
        assert_eq!(
            parse_state_selector("Todo"),
            Some(StateSelector::Name("Todo".to_string()))
        );
        assert_eq!(parse_state_selector("   "), None);
    }

    #[test]
    fn resolves_state_by_name_case_insensitively() {
        let items = vec![
            WorkflowState {
                id: "s1".to_string(),
                name: "Todo".to_string(),
                state_type: "unstarted".to_string(),
            },
            WorkflowState {
                id: "s2".to_string(),
                name: "Done".to_string(),
                state_type: "completed".to_string(),
            },
        ];
        let found = match_state(&StateSelector::Name("todo".to_string()), &items);
        match found {
            StateResolution::Resolved(item) => assert_eq!(item.id, "s1"),
            other => panic!("expected resolved, got {other:?}"),
        }
        let missing = match_state(&StateSelector::Name("nope".to_string()), &items);
        assert!(matches!(missing, StateResolution::NotFound(_)));
    }

    #[test]
    fn team_key_from_identifier_parses_key_number() {
        assert_eq!(team_key_from_identifier("GUZ-22"), Some("GUZ"));
        assert_eq!(team_key_from_identifier("  ENG-1  "), Some("ENG"));
        assert_eq!(
            team_key_from_identifier("e3b567ba-bcd1-42d8-8ae8-129fd11c97ab"),
            None
        );
        assert_eq!(team_key_from_identifier("i1"), None);
        assert_eq!(team_key_from_identifier("GUZ-"), None);
        assert_eq!(team_key_from_identifier("guz-22"), None);
    }

    #[test]
    fn matches_label_by_name_case_insensitively() {
        let items = vec![
            Label {
                id: "l1".to_string(),
                name: "docs".to_string(),
            },
            Label {
                id: "l2".to_string(),
                name: "api".to_string(),
            },
        ];
        let found = match_label(&LabelSelector::Name("DOCS".to_string()), &items);
        match found {
            LabelResolution::Resolved(item) => assert_eq!(item.id, "l1"),
            other => panic!("expected resolved, got {other:?}"),
        }
        let missing = match_label(&LabelSelector::Name("nope".to_string()), &items);
        assert!(matches!(missing, LabelResolution::NotFound(_)));
        assert_eq!(format_label_candidates(&items), "docs, api");
        assert_eq!(parse_label_selector("   "), None);
    }
}
