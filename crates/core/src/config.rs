use crate::gateway;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub gateway: gateway::Config,
    pub atlassian: Atlassian,
    pub linear: Linear,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Linear {
    pub api_key: Option<String>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Atlassian {
    pub base_url: Option<String>,
    pub email: Option<String>,
    pub api_token: Option<String>,
    pub jira: Jira,
    pub confluence: Confluence,
    pub bitbucket: Bitbucket,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Jira {
    pub base_url: Option<String>,
    pub email: Option<String>,
    pub api_token: Option<String>,
    pub board_id: Option<u64>,
    pub query: Option<String>,
    pub issue_key: Option<String>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Confluence {
    pub base_url: Option<String>,
    pub email: Option<String>,
    pub api_token: Option<String>,
    pub query: Option<String>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Bitbucket {
    pub base_url: Option<String>,
    pub username: Option<String>,
    pub app_password: Option<String>,
}

pub fn resolve<T>(
    service: Option<T>,
    shared: Option<T>,
    file_service: Option<T>,
    file_shared: Option<T>,
) -> Option<T> {
    service.or(shared).or(file_service).or(file_shared)
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);

pub fn config_path(
    override_path: Option<&OsStr>,
    xdg_config_home: Option<&OsStr>,
    home: Option<&Path>,
    cwd: &Path,
) -> Result<PathBuf, Error> {
    optional_config_path(override_path, xdg_config_home, home, cwd).ok_or_else(|| {
        Error("Cannot resolve config path: set HOME, XDG_CONFIG_HOME or MCPTOOLS_CONFIG".into())
    })
}

pub fn optional_config_path(
    override_path: Option<&OsStr>,
    xdg_config_home: Option<&OsStr>,
    home: Option<&Path>,
    cwd: &Path,
) -> Option<PathBuf> {
    if let Some(path) = override_path.filter(|path| !path.is_empty()).map(Path::new) {
        return Some(if path.is_absolute() {
            path.to_owned()
        } else {
            cwd.join(path)
        });
    }
    let directory = xdg_config_home
        .map(Path::new)
        .filter(|path| path.is_absolute())
        .map(Path::to_owned)
        .or_else(|| {
            home.filter(|path| !path.as_os_str().is_empty())
                .map(|home| home.join(".config"))
        })?;
    Some(directory.join("mcptools/config.toml"))
}

pub fn cache_path(config_path: &Path) -> Result<PathBuf, Error> {
    let path = config_path.with_file_name("upstreams.cache.json");
    if path == config_path {
        return Err(Error(
            "Config file cannot be named upstreams.cache.json".into(),
        ));
    }
    Ok(path)
}

pub fn parse(text: &str) -> Result<AppConfig, Error> {
    let config: AppConfig = toml::from_str(text).map_err(|error: toml::de::Error| {
        let offset = error.span().map_or(0, |span| span.start.min(text.len()));
        let prefix = text.get(..offset).unwrap_or(text);
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        let message = if error.message().starts_with("invalid type:") {
            error
                .message()
                .split_once(", expected ")
                .map_or("invalid field type".to_string(), |(_, expected)| {
                    format!("invalid field type; expected {expected}")
                })
        } else {
            error.message().to_string()
        };
        Error(format!("TOML at line {line}, column {column}: {message}"))
    })?;
    gateway::validate_config(&config.gateway).map_err(|error| Error(error.to_string()))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_and_section_layer_onto_defaults() {
        for text in ["", "[gateway]"] {
            assert!(parse(text).unwrap().gateway.upstreams.is_empty());
        }
        assert!(parse("{\"upstreams\": []}").is_err());
    }

    #[test]
    fn relative_xdg_and_empty_override_use_home() {
        assert_eq!(
            config_path(
                Some(OsStr::new("")),
                Some(OsStr::new("relative")),
                Some(Path::new("/home/test")),
                Path::new("/cwd")
            )
            .unwrap(),
            Path::new("/home/test/.config/mcptools/config.toml")
        );
        assert!(config_path(None, None, None, Path::new("/cwd")).is_err());
        assert_eq!(
            optional_config_path(None, None, None, Path::new("/cwd")),
            None
        );
    }

    #[test]
    fn type_errors_do_not_echo_secret_values() {
        let error =
            parse("[[gateway.upstreams]]\nprefix = 'a'\ncommand = 'x'\nargs = 'secret-value'\n")
                .err()
                .unwrap();
        assert!(!error.to_string().contains("secret-value"));
        assert!(error.to_string().contains("line 4"));
    }
}

#[cfg(test)]
mod atlassian_regressions {
    use super::*;

    #[test]
    fn file_only_and_service_over_shared() {
        let config = parse("[atlassian]\nbase_url = 'shared'\nemail = 'shared-email'\napi_token = 'shared-token'\n[atlassian.jira]\nbase_url = 'service'\nboard_id = 42\n").unwrap();
        let value = toml::Value::try_from(config).unwrap();
        assert_eq!(value["atlassian"]["base_url"].as_str(), Some("shared"));
        assert_eq!(
            value["atlassian"]["jira"]["base_url"].as_str(),
            Some("service")
        );
        assert_eq!(
            value["atlassian"]["jira"]["board_id"].as_integer(),
            Some(42)
        );
    }

    #[test]
    fn atlassian_validation_rejects_unknown_fields_without_leaking_secrets() {
        let error = parse("[atlassian.jira]\nunknown = 'secret-value'\n")
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("unknown"));
        assert!(!error.contains("secret-value"));
    }
    #[test]
    fn environment_precedes_file_and_service_precedes_shared() {
        assert_eq!(
            resolve(
                Some("service-env"),
                Some("shared-env"),
                Some("service-file"),
                Some("shared-file")
            ),
            Some("service-env")
        );
        assert_eq!(
            resolve(
                None,
                Some("shared-env"),
                Some("service-file"),
                Some("shared-file")
            ),
            Some("shared-env")
        );
        assert_eq!(
            resolve(None, None, Some("service-file"), Some("shared-file")),
            Some("service-file")
        );
        assert_eq!(
            resolve(None, None, None, Some("shared-file")),
            Some("shared-file")
        );
    }

    #[test]
    fn operation_defaults_are_parsed() {
        let config = parse("[atlassian.jira]\nquery = 'project = TEST'\nissue_key = 'TEST-1'\n[atlassian.confluence]\nquery = 'space = TEST'\n").unwrap();
        let value = toml::Value::try_from(config).unwrap();
        assert_eq!(
            value["atlassian"]["jira"]["query"].as_str(),
            Some("project = TEST")
        );
        assert_eq!(
            value["atlassian"]["jira"]["issue_key"].as_str(),
            Some("TEST-1")
        );
        assert_eq!(
            value["atlassian"]["confluence"]["query"].as_str(),
            Some("space = TEST")
        );
    }
}

#[cfg(test)]
mod linear_regressions {
    use super::*;

    #[test]
    fn linear_file_key_and_environment_precedence() {
        let config = parse("[linear]\napi_key = 'file-key'\n").unwrap();
        let value = toml::Value::try_from(config).unwrap();
        let key = value["linear"]["api_key"].as_str();
        assert_eq!(resolve(None, None, key, None), Some("file-key"));
        assert_eq!(resolve(Some("env-key"), None, key, None), Some("env-key"));
    }

    #[test]
    fn linear_invalid_fields_do_not_reveal_values() {
        for (text, expected) in [
            ("[linear]\nunknown = 'secret-value'\n", "unknown"),
            (
                "[linear]\napi_key = ['secret-value']\n",
                "expected a string",
            ),
        ] {
            let error = parse(text).err().unwrap().to_string();
            assert!(error.contains(expected), "{error}");
            assert!(!error.contains("secret-value"));
        }
    }
}
