use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct Template {
    pub name: String,
    pub description: Option<String>,
    pub system: Option<String>,
    pub template: String,
    #[serde(default)]
    pub default_vars: Value,
}

#[derive(Debug, Clone)]
pub struct LoadedTemplate {
    pub template: Template,
    pub raw: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    #[error(
        "template '{name}' is not installed; run `mcptools atlas setup` to install atlas templates"
    )]
    Missing { name: String },
    #[error("malformed template {}: {reason}", path.display())]
    Malformed { path: PathBuf, reason: String },
    #[error("failed to render template '{name}': {reason}")]
    Render { name: String, reason: String },
}

pub fn load(templates_dir: &Path, name: &str) -> Result<LoadedTemplate, TemplateError> {
    let path = templates_dir.join(format!("{name}.toml"));
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(TemplateError::Missing {
                name: name.to_string(),
            })
        }
        Err(e) => {
            return Err(TemplateError::Malformed {
                path,
                reason: e.to_string(),
            })
        }
    };
    parse(&path, &raw)
}

pub fn validate(toml_str: &str) -> Result<Template, String> {
    let template: Template = toml::from_str(toml_str).map_err(|e| e.to_string())?;
    if !(template.default_vars.is_null() || template.default_vars.is_object()) {
        return Err("default_vars must be a table".to_string());
    }
    Ok(template)
}

fn parse(path: &Path, raw: &str) -> Result<LoadedTemplate, TemplateError> {
    let template = validate(raw).map_err(|reason| TemplateError::Malformed {
        path: path.to_path_buf(),
        reason,
    })?;
    Ok(LoadedTemplate {
        template,
        raw: raw.to_string(),
    })
}

pub fn render(t: &LoadedTemplate, vars: &Value) -> Result<String, TemplateError> {
    render_body(&t.template.name, &t.template.template, t, vars)
}

pub fn render_system(t: &LoadedTemplate, vars: &Value) -> Result<String, TemplateError> {
    match &t.template.system {
        Some(system) => render_body(&t.template.name, system, t, vars),
        None => Ok(String::new()),
    }
}

fn render_body(
    name: &str,
    body: &str,
    t: &LoadedTemplate,
    vars: &Value,
) -> Result<String, TemplateError> {
    let render_error = |reason: String| TemplateError::Render {
        name: name.to_string(),
        reason,
    };
    let context = tera::Context::from_value(merge_vars(&t.template.default_vars, vars))
        .map_err(|e| render_error(e.to_string()))?;
    let mut tera = tera::Tera::default();
    tera.add_raw_template(name, body)
        .map_err(|e| render_error(e.to_string()))?;
    tera.render(name, &context)
        .map_err(|e| render_error(e.to_string()))
}

fn merge_vars(default_vars: &Value, vars: &Value) -> Value {
    let mut merged = match default_vars {
        Value::Object(map) => map.clone(),
        _ => serde_json::Map::new(),
    };
    if let Value::Object(map) = vars {
        for (key, value) in map {
            merged.insert(key.clone(), value.clone());
        }
    }
    Value::Object(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const ATLAS_FILE_TOML: &str =
        include_str!("../../../mcptools/src/atlas/templates/atlas-file.toml");
    const ATLAS_DIR_TOML: &str =
        include_str!("../../../mcptools/src/atlas/templates/atlas-dir.toml");
    const ATLAS_PRIMER_TOML: &str =
        include_str!("../../../mcptools/src/atlas/templates/atlas-primer.toml");

    const INSTALLED: [(&str, &str); 3] = [
        ("atlas-file", ATLAS_FILE_TOML),
        ("atlas-dir", ATLAS_DIR_TOML),
        ("atlas-primer", ATLAS_PRIMER_TOML),
    ];

    fn write_template(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(format!("{name}.toml")), body).unwrap();
    }

    fn install_embedded(dir: &Path) {
        for (name, raw) in INSTALLED {
            write_template(dir, name, raw);
        }
    }

    fn template_vars_cases() -> Vec<(&'static str, Value)> {
        vec![
            (
                "atlas-file",
                serde_json::json!({
                    "primer": "Project \"mcptools\" — a Rust CLI 🦀 with {{double_braces}}",
                    "location": "src/atlas.rs:42",
                    "symbols": "parse, render, {{ not_a_var }}",
                    "content": "let quoted = \"a\\\"b\";\nfn main() {}\n{{ literal_braces }}"
                }),
            ),
            (
                "atlas-dir",
                serde_json::json!({
                    "primer": "Primer with \"quotes\" and 🦀",
                    "dir_path": "src/atlas/",
                    "children": "templates/ \"quoted\" {{ inner }}",
                    "symbols": "list, describe, {{ inner_two }}"
                }),
            ),
            (
                "atlas-primer",
                serde_json::json!({
                    "raw_primer": "raw context with \"quotes\", 🦀 and {{ braces }}"
                }),
            ),
        ]
    }

    #[test]
    fn template_vars_cases_cover_installed_template_vars() {
        let dir = tempfile::tempdir().unwrap();
        install_embedded(dir.path());
        let cases = template_vars_cases();
        assert_eq!(cases.len(), INSTALLED.len());
        for (name, vars) in &cases {
            let loaded = load(dir.path(), name).unwrap();
            let provided = vars.as_object().unwrap();
            let defaults = loaded.template.default_vars.as_object().unwrap();
            for key in provided.keys() {
                assert!(defaults.contains_key(key), "{name} lacks var {key}");
            }
            for key in defaults.keys() {
                assert!(
                    provided.contains_key(key),
                    "case for {name} lacks var {key}"
                );
            }
        }
    }

    #[test]
    fn installed_templates_render_hostile_vars_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        install_embedded(dir.path());
        for (name, vars) in template_vars_cases() {
            let loaded = load(dir.path(), name).unwrap();
            let out = render(&loaded, &vars).unwrap();
            for (key, value) in vars.as_object().unwrap() {
                let text = value.as_str().unwrap();
                assert!(
                    out.contains(text),
                    "{name} lost var {key}: {text:?} vs {out:?}"
                );
            }
        }
    }

    #[test]
    fn installed_templates_render_with_empty_vars_through_defaults() {
        let dir = tempfile::tempdir().unwrap();
        install_embedded(dir.path());
        let file = load(dir.path(), "atlas-file").unwrap();
        assert!(render(&file, &serde_json::json!({}))
            .unwrap()
            .contains("# File Content"));
        let dir_t = load(dir.path(), "atlas-dir").unwrap();
        assert!(render(&dir_t, &serde_json::json!({}))
            .unwrap()
            .contains("# Directory:"));
        let primer = load(dir.path(), "atlas-primer").unwrap();
        assert!(render(&primer, &serde_json::json!({}))
            .unwrap()
            .contains("concise mental model"));
    }

    #[test]
    fn render_system_carries_format_contract_examples() {
        let dir = tempfile::tempdir().unwrap();
        install_embedded(dir.path());
        let file = load(dir.path(), "atlas-file").unwrap();
        let file_system = render_system(&file, &serde_json::json!({})).unwrap();
        assert!(file_system.contains("SHORT: CLI argument parser and validation"));
        assert!(file_system.contains("LONG: Defines the command-line interface using clap"));
        let dir_t = load(dir.path(), "atlas-dir").unwrap();
        let dir_system = render_system(&dir_t, &serde_json::json!({})).unwrap();
        assert!(dir_system.contains("SHORT: Database access layer and query builders"));
        assert!(dir_system.contains("LONG: Contains the SQLite connection wrapper"));
        let primer = load(dir.path(), "atlas-primer").unwrap();
        assert_eq!(render_system(&primer, &serde_json::json!({})).unwrap(), "");
    }

    #[test]
    fn render_one_megabyte_content_stays_exact() {
        let dir = tempfile::tempdir().unwrap();
        install_embedded(dir.path());
        let loaded = load(dir.path(), "atlas-file").unwrap();
        let (name, mut vars) = template_vars_cases().remove(0);
        assert_eq!(name, "atlas-file");
        let mut content = String::with_capacity(1 << 20);
        content.push_str("START é 🦀 \"quote\" {{ brace }}\n");
        content.extend(std::iter::repeat_n('x', 1 << 20));
        content.push_str("\nEND");
        vars["content"] = Value::String(content);
        let out = render(&loaded, &vars).unwrap();
        assert!(out.starts_with("# Project Context\n"));
        assert!(out.contains("START é 🦀 \"quote\" {{ brace }}\n"));
        assert!(out.ends_with("\nEND\n\nDescribe this file.\n"));
    }

    #[test]
    fn corrupted_installed_template_reports_malformed() {
        let dir = tempfile::tempdir().unwrap();
        let truncated = &ATLAS_FILE_TOML[..ATLAS_FILE_TOML.len() / 2];
        write_template(dir.path(), "atlas-file", truncated);
        let err = load(dir.path(), "atlas-file").unwrap_err();
        match err {
            TemplateError::Malformed { path, .. } => {
                assert_eq!(path, dir.path().join("atlas-file.toml"));
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn merge_vars_caller_wins_and_defaults_fill_absent() {
        let defaults = serde_json::json!({"primer": "", "content": "fallback"});
        let caller = serde_json::json!({"primer": "real"});
        let merged = merge_vars(&defaults, &caller);
        assert_eq!(merged["primer"], "real");
        assert_eq!(merged["content"], "fallback");
    }

    #[test]
    fn load_missing_template_reports_missing() {
        let dir = tempfile::tempdir().unwrap();
        let err = load(dir.path(), "atlas-file").unwrap_err();
        assert!(matches!(&err, TemplateError::Missing { name } if name == "atlas-file"));
        assert!(err.to_string().contains("mcptools atlas setup"));
    }

    #[test]
    fn load_malformed_template_names_the_file() {
        let dir = tempfile::tempdir().unwrap();
        write_template(dir.path(), "atlas-file", "not = [valid");
        let err = load(dir.path(), "atlas-file").unwrap_err();
        match err {
            TemplateError::Malformed { path, .. } => {
                assert_eq!(path, dir.path().join("atlas-file.toml"));
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn render_falls_back_to_default_vars_and_carries_raw() {
        let dir = tempfile::tempdir().unwrap();
        write_template(
            dir.path(),
            "atlas-file",
            "name = \"atlas-file\"\ntemplate = \"a={{ x }} b={{ y }}\"\ndefault_vars = { x = \"\", y = \"keep\" }\n",
        );
        let loaded = load(dir.path(), "atlas-file").unwrap();
        assert!(loaded.raw.contains("default_vars"));
        let out = render(&loaded, &serde_json::json!({"x": "set"})).unwrap();
        assert_eq!(out, "a=set b=keep");
    }

    #[test]
    fn load_malformed_default_vars_names_the_file() {
        let dir = tempfile::tempdir().unwrap();
        write_template(
            dir.path(),
            "atlas-file",
            "name = \"atlas-file\"\ntemplate = \"body\"\ndefault_vars = \"nope\"\n",
        );
        let err = load(dir.path(), "atlas-file").unwrap_err();
        match err {
            TemplateError::Malformed { path, .. } => {
                assert_eq!(path, dir.path().join("atlas-file.toml"));
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn render_system_is_empty_when_system_absent() {
        let dir = tempfile::tempdir().unwrap();
        write_template(
            dir.path(),
            "atlas-file",
            "name = \"atlas-file\"\ntemplate = \"body\"\n",
        );
        let loaded = load(dir.path(), "atlas-file").unwrap();
        assert_eq!(render_system(&loaded, &serde_json::json!({})).unwrap(), "");
    }
}
