use super::{is_uuid, parse_project_selector, ProjectSelector, ProjectStatus, User};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};

fn supplied<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectUpdateArgs {
    #[schemars(description = "Project UUID or name; names require team")]
    pub id: String,
    #[serde(default, deserialize_with = "supplied")]
    #[schemars(description = "Team id, key, or name for project name resolution")]
    pub team: Option<String>,
    #[serde(default, deserialize_with = "supplied")]
    #[schemars(description = "New project name; must not be blank")]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "supplied")]
    #[schemars(description = "Short description; clearDescription sets an empty string")]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "supplied")]
    #[schemars(
        description = "Markdown content; exact whitespace preserved; clearContent sets an empty string"
    )]
    pub content: Option<String>,
    #[serde(default, deserialize_with = "supplied")]
    #[schemars(
        description = "Workspace project status UUID or case-insensitive name; cannot be cleared"
    )]
    pub status: Option<String>,
    #[serde(default, deserialize_with = "supplied")]
    #[schemars(description = "Lead user UUID or me; clearLead sets null")]
    pub lead: Option<String>,
    #[serde(default, deserialize_with = "supplied")]
    #[schemars(description = "Start date YYYY-MM-DD; clearStartDate sets null")]
    pub start_date: Option<String>,
    #[serde(default, deserialize_with = "supplied")]
    #[schemars(description = "Target date YYYY-MM-DD; clearTargetDate sets null")]
    pub target_date: Option<String>,
    #[serde(default, deserialize_with = "supplied")]
    #[schemars(
        range(min = 0, max = 4),
        description = "Priority 0–4; 0 resets priority"
    )]
    pub priority: Option<u8>,
    #[serde(default)]
    #[schemars(description = "Set description to empty string; conflicts with description")]
    pub clear_description: bool,
    #[serde(default)]
    #[schemars(description = "Set content to empty string; conflicts with content")]
    pub clear_content: bool,
    #[serde(default)]
    #[schemars(description = "Set lead to null; conflicts with lead")]
    pub clear_lead: bool,
    #[serde(default)]
    #[schemars(description = "Set startDate to null; conflicts with startDate")]
    pub clear_start_date: bool,
    #[serde(default)]
    #[schemars(description = "Set targetDate to null; conflicts with targetDate")]
    pub clear_target_date: bool,
}

pub fn project_update_input(args: &ProjectUpdateArgs) -> Result<Value, String> {
    validate_project_selector(&args.id)?;
    if matches!(
        parse_project_selector(&args.id),
        Some(ProjectSelector::Name(_))
    ) && args.team.is_none()
    {
        return Err("Project names require team".into());
    }
    for (field, value) in [
        ("team", &args.team),
        ("name", &args.name),
        ("status", &args.status),
        ("lead", &args.lead),
    ] {
        if value.as_ref().is_some_and(|text| text.trim().is_empty()) {
            return Err(format!("{field} must not be blank"));
        }
    }
    for selector in [&args.team, &args.status].into_iter().flatten() {
        validate_project_selector(selector)?;
    }
    if args
        .lead
        .as_ref()
        .is_some_and(|value| !is_uuid(value.trim()) && !value.trim().eq_ignore_ascii_case("me"))
    {
        return Err("lead must be a user UUID or me".into());
    }
    if args.priority.is_some_and(|value| value > 4) {
        return Err("priority must be between 0 and 4".into());
    }
    let mut input = json!({});
    for (field, value, clear, cleared) in [
        ("name", &args.name, false, Value::Null),
        (
            "description",
            &args.description,
            args.clear_description,
            json!(""),
        ),
        ("content", &args.content, args.clear_content, json!("")),
        ("leadId", &args.lead, args.clear_lead, Value::Null),
        (
            "startDate",
            &args.start_date,
            args.clear_start_date,
            Value::Null,
        ),
        (
            "targetDate",
            &args.target_date,
            args.clear_target_date,
            Value::Null,
        ),
        ("statusId", &args.status, false, Value::Null),
    ] {
        if clear && value.is_some() {
            return Err(format!("{field} cannot be set and cleared together"));
        }
        if let Some(value) = value {
            if ["startDate", "targetDate"].contains(&field) && !valid_project_date(value) {
                return Err(format!("{field} must be a valid YYYY-MM-DD calendar date"));
            }
            input[field] = json!(if field == "content" {
                value.as_str()
            } else {
                value.trim()
            });
        } else if clear {
            input[field] = cleared;
        }
    }
    if let Some(priority) = args.priority {
        input["priority"] = json!(priority);
    }
    if input.as_object().is_none_or(|fields| fields.is_empty()) {
        return Err("Project update needs at least one changed property".into());
    }
    Ok(input)
}

fn validate_project_selector(value: &str) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().any(char::is_control)
        || (value.len() == 36
            && value.bytes().filter(|byte| *byte == b'-').count() == 4
            && !is_uuid(value))
    {
        return Err("Malformed project, team, or status selector".into());
    }
    Ok(())
}

fn valid_project_date(value: &str) -> bool {
    value.len() == 10
        && value.bytes().enumerate().all(|(index, byte)| {
            if [4, 7].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
        && !value.starts_with("0000")
        && chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
}

pub fn resolve_project_status(
    selector: &str,
    statuses: &[ProjectStatus],
) -> Result<String, String> {
    let selector = selector.trim();
    let matches: Vec<_> = statuses
        .iter()
        .filter(|status| {
            if is_uuid(selector) {
                status.id.eq_ignore_ascii_case(selector)
            } else {
                status.name.eq_ignore_ascii_case(selector)
            }
        })
        .collect();
    match matches.as_slice() {
        [status] if !status.id.trim().is_empty() => Ok(status.id.clone()),
        [_] => Err("Malformed project status identity".into()),
        [] => Err(format!("Project status not found: {selector}")),
        _ => Err(format!(
            "Ambiguous project status '{selector}'. Candidates: {}",
            matches
                .iter()
                .map(|status| format!("{} ({})", status.name, status.id))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct UpdatedProjectStatus {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub status_type: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdateOutput {
    pub id: String,
    pub name: String,
    pub url: String,
    pub description: String,
    pub content: Option<String>,
    pub status: UpdatedProjectStatus,
    pub lead: Option<User>,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
    pub priority: u8,
}

pub fn transform_project_update(data: Value) -> Result<ProjectUpdateOutput, String> {
    let payload = &data["projectUpdate"];
    if payload["success"].as_bool() != Some(true) {
        return Err("Linear projectUpdate failed: success was not true".into());
    }
    let project = &payload["project"];
    for field in [
        "id",
        "name",
        "url",
        "description",
        "content",
        "status",
        "lead",
        "startDate",
        "targetDate",
        "priority",
    ] {
        if project.get(field).is_none() {
            return Err(format!(
                "Linear projectUpdate missing project field: {field}"
            ));
        }
    }
    if project["lead"].is_object() && project["lead"].get("email").is_none() {
        return Err("Linear projectUpdate missing lead email".into());
    }
    let output: ProjectUpdateOutput = serde_json::from_value(project.clone())
        .map_err(|error| format!("Malformed projectUpdate project: {error}"))?;
    if [
        &output.id,
        &output.name,
        &output.url,
        &output.status.id,
        &output.status.name,
        &output.status.status_type,
    ]
    .iter()
    .any(|value| value.trim().is_empty())
        || output.priority > 4
        || [&output.start_date, &output.target_date]
            .into_iter()
            .flatten()
            .any(|value| !valid_project_date(value))
        || output
            .lead
            .as_ref()
            .is_some_and(|user| user.id.trim().is_empty() || user.name.trim().is_empty())
    {
        return Err("Malformed projectUpdate project properties".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "12345678-1234-1234-1234-123456789abc";

    fn args(fields: Value) -> ProjectUpdateArgs {
        let mut input = json!({"id": ID});
        input
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        serde_json::from_value(input).unwrap()
    }

    #[test]
    fn partial_updates_preserve_omissions_whitespace_and_clear_values() {
        for (fields, expected) in [
            (json!({"name": " New "}), json!({"name": "New"})),
            (
                json!({"description": " Summary "}),
                json!({"description": "Summary"}),
            ),
            (
                json!({"content": "    code\n\n# Heading\n"}),
                json!({"content": "    code\n\n# Heading\n"}),
            ),
            (json!({"priority": 0}), json!({"priority": 0})),
            (
                json!({"clearDescription": true}),
                json!({"description": ""}),
            ),
            (json!({"clearContent": true}), json!({"content": ""})),
            (json!({"clearLead": true}), json!({"leadId": null})),
            (json!({"clearStartDate": true}), json!({"startDate": null})),
            (
                json!({"clearTargetDate": true}),
                json!({"targetDate": null}),
            ),
            (
                json!({"startDate": "2028-02-29", "targetDate": "2028-03-01", "lead": ID, "status": "Custom", "priority": 4}),
                json!({"startDate": "2028-02-29", "targetDate": "2028-03-01", "leadId": ID, "statusId": "Custom", "priority": 4}),
            ),
        ] {
            assert_eq!(project_update_input(&args(fields)).unwrap(), expected);
        }
        let mut named = args(json!({"name": "New", "team": "GUZ"}));
        named.id = "Existing".into();
        assert_eq!(
            project_update_input(&named).unwrap(),
            json!({"name": "New"})
        );
    }

    #[test]
    fn invalid_inputs_and_clear_conflicts_reject() {
        for fields in [
            json!({}),
            json!({"clearLead": false}),
            json!({"name": " \n"}),
            json!({"team": " " , "priority": 1}),
            json!({"status": " "}),
            json!({"lead": "Ada"}),
            json!({"lead": "-123456781234-1234-1234-123456789abc"}),
            json!({"priority": 5}),
            json!({"id": "Existing", "name": "New"}),
            json!({"id": " ", "name": "New"}),
            json!({"status": "Bad\nSelector"}),
            json!({"description": "", "clearDescription": true}),
            json!({"content": "", "clearContent": true}),
            json!({"lead": "me", "clearLead": true}),
            json!({"startDate": "2026-01-01", "clearStartDate": true}),
            json!({"targetDate": "2026-01-01", "clearTargetDate": true}),
        ] {
            assert!(
                project_update_input(&args(fields.clone())).is_err(),
                "{fields}"
            );
        }
        for date in [
            "2026-02-29",
            "2026-04-31",
            "2026-13-01",
            "2026-00-01",
            "2026-01-00",
            "2026-1-01",
            "2026-01-01T00:00:00Z",
            "0000-01-01",
            " 2026-01-01",
        ] {
            for field in ["startDate", "targetDate"] {
                assert!(
                    project_update_input(&args(json!({field: date}))).is_err(),
                    "{date}"
                );
            }
        }
        for fields in [
            json!({"priority": -1}),
            json!({"priority": 1.5}),
            json!({"priority": "1"}),
            json!({"clearStatus": true}),
            json!({"name": null}),
            json!({"status": null}),
            json!({"lead": null}),
            json!({"startDate": null}),
            json!({"clearLead": null}),
        ] {
            let mut input = json!({"id": ID});
            input
                .as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            assert!(serde_json::from_value::<ProjectUpdateArgs>(input).is_err());
        }
    }

    #[test]
    fn statuses_use_workspace_names_and_reject_ambiguity() {
        let status = ProjectStatus {
            id: ID.into(),
            name: "Custom".into(),
            status_type: "planned".into(),
            color: "#000000".into(),
            position: 1.0,
            description: None,
        };
        assert_eq!(
            resolve_project_status(" custom ", std::slice::from_ref(&status)).unwrap(),
            ID
        );
        assert_eq!(
            resolve_project_status(ID, std::slice::from_ref(&status)).unwrap(),
            ID
        );
        assert_eq!(
            resolve_project_status(&ID.to_uppercase(), std::slice::from_ref(&status)).unwrap(),
            ID
        );
        assert!(
            resolve_project_status("Missing", std::slice::from_ref(&status))
                .unwrap_err()
                .contains("not found")
        );
        let mut duplicate = status.clone();
        duplicate.id = "other".into();
        assert!(resolve_project_status("Custom", &[status, duplicate])
            .unwrap_err()
            .contains("Ambiguous"));
    }

    #[test]
    fn uuid_selectors_require_canonical_separator_positions() {
        assert!(is_uuid(ID));
        for invalid in [
            "-123456781234-1234-1234-123456789abc",
            "12345678-1234-1234-1234-123456789abg",
            "12345678123412341234123456789abc",
        ] {
            assert!(!is_uuid(invalid));
        }
        assert!(project_update_input(&args(
            json!({"id": "-123456781234-1234-1234-123456789abc", "team": "GUZ", "name": "New"})
        ))
        .is_err());
    }

    #[test]
    fn response_requires_success_and_every_selected_property() {
        let project = json!({"id": ID, "name": "New", "url": "https://linear.app/project/new", "description": "", "content": null, "status": {"id": ID, "name": "Custom", "type": "planned"}, "lead": null, "startDate": null, "targetDate": null, "priority": 0});
        assert!(transform_project_update(
            json!({"projectUpdate": {"success": true, "project": project}})
        )
        .is_ok());
        for field in project.as_object().unwrap().keys() {
            let mut missing = project.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                transform_project_update(
                    json!({"projectUpdate": {"success": true, "project": missing}})
                )
                .is_err(),
                "{field}"
            );
        }
        for (field, value) in [
            ("id", json!(1)),
            ("name", json!(" ")),
            ("url", Value::Null),
            ("description", Value::Null),
            ("status", Value::Null),
            ("status", json!({"id": ID, "name": "Custom"})),
            ("lead", json!({"id": ID, "name": "Ada"})),
            ("content", json!(1)),
            ("startDate", json!("2026-02-29")),
            ("targetDate", json!(3)),
            ("priority", json!(5)),
        ] {
            let mut invalid = project.clone();
            invalid[field] = value;
            assert!(
                transform_project_update(
                    json!({"projectUpdate": {"success": true, "project": invalid}})
                )
                .is_err(),
                "{field}"
            );
        }
        for payload in [
            Value::Null,
            json!({}),
            json!({"success": false, "project": project}),
            json!({"success": "true", "project": project}),
            json!({"success": true}),
            json!({"success": true, "project": null}),
        ] {
            assert!(transform_project_update(json!({"projectUpdate": payload})).is_err());
        }
    }
}
