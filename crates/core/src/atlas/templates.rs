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

fn parse(path: &Path, raw: &str) -> Result<LoadedTemplate, TemplateError> {
    let malformed = |reason: String| TemplateError::Malformed {
        path: path.to_path_buf(),
        reason,
    };
    let template: Template = toml::from_str(raw).map_err(|e| malformed(e.to_string()))?;
    if !(template.default_vars.is_null() || template.default_vars.is_object()) {
        return Err(malformed("default_vars must be a table".to_string()));
    }
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

    fn write_template(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(format!("{name}.toml")), body).unwrap();
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
