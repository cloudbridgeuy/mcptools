use crate::gateway;
use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub gateway: gateway::Config,
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
    if let Some(path) = override_path.filter(|path| !path.is_empty()).map(Path::new) {
        return Ok(if path.is_absolute() {
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
        })
        .ok_or_else(|| {
            Error("Cannot resolve config path: set HOME, XDG_CONFIG_HOME or MCPTOOLS_CONFIG".into())
        })?;
    Ok(directory.join("mcptools/config.toml"))
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
