use super::types::{Project, Team};
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
        && value.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
        && value.chars().filter(|c| *c == '-').count() == 4
}

fn is_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 10
        && value
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
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
}
