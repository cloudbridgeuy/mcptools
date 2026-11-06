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
    pub jev: Jev,
    pub images: Images,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Images {
    pub api: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub mainline: Option<String>,
    pub model: Option<String>,
    pub size: Option<String>,
    pub quality: Option<String>,
}

impl Images {
    pub fn with_fallback(mut self, fallback: Self) -> Self {
        self.api = self.api.or(fallback.api);
        self.api_key = self.api_key.or(fallback.api_key);
        self.base_url = self.base_url.or(fallback.base_url);
        self.mainline = self.mainline.or(fallback.mainline);
        self.model = self.model.or(fallback.model);
        self.size = self.size.or(fallback.size);
        self.quality = self.quality.or(fallback.quality);
        self
    }

    pub fn needs_file(&self) -> bool {
        let backend_missing = match self.api.as_deref() {
            Some("openai") => self.api_key.is_none() || self.base_url.is_none(),
            Some("chatgpt") => self.mainline.is_none(),
            None => return true,
            Some(_) => return false,
        };
        backend_missing || self.model.is_none() || self.size.is_none() || self.quality.is_none()
    }

    pub fn resolve(self, file: Self) -> Result<ResolvedImages, Error> {
        use crate::images::{CHAT_MAINLINE_DEFAULT, DEFAULT_BASE_URL, DEFAULT_MODEL};
        let values = self.with_fallback(file);
        let backend = match values.api.as_deref().unwrap_or("chatgpt") {
            "chatgpt" => ImageBackend::ChatGpt {
                mainline: values
                    .mainline
                    .unwrap_or_else(|| CHAT_MAINLINE_DEFAULT.into()),
            },
            "openai" => ImageBackend::OpenAi {
                api_key: values
                    .api_key
                    .filter(|key| !key.is_empty())
                    .ok_or_else(|| {
                        Error(
                            "Missing OpenAI API key. Set OPENAI_API_KEY or pass --api-key.".into(),
                        )
                    })?,
                base_url: values.base_url.unwrap_or_else(|| DEFAULT_BASE_URL.into()),
            },
            _ => return Err(Error("images.api must be chatgpt or openai".into())),
        };
        Ok(ResolvedImages {
            backend,
            model: values.model.unwrap_or_else(|| DEFAULT_MODEL.into()),
            size: values.size.unwrap_or_else(|| "auto".into()),
            quality: values.quality.unwrap_or_else(|| "auto".into()),
        })
    }
}

pub enum ImageBackend {
    ChatGpt { mainline: String },
    OpenAi { api_key: String, base_url: String },
}

pub struct ResolvedImages {
    pub backend: ImageBackend,
    pub model: String,
    pub size: String,
    pub quality: String,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Jev {
    pub provider: Option<String>,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
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
    if let Some(provider) = &config.jev.provider {
        crate::jev::parse_provider(provider).map_err(|error| Error(error.to_string()))?;
    }
    if config
        .images
        .api
        .as_deref()
        .is_some_and(|value| !matches!(value, "openai" | "chatgpt"))
    {
        return Err(Error("images.api must be chatgpt or openai".into()));
    }
    if config
        .images
        .size
        .as_deref()
        .is_some_and(|value| crate::images::validate_size(value).is_err())
    {
        return Err(Error("invalid images.size".into()));
    }
    if config
        .images
        .quality
        .as_deref()
        .is_some_and(|value| crate::images::validate_quality(value).is_err())
    {
        return Err(Error("invalid images.quality".into()));
    }
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

#[cfg(test)]
mod jev_regressions {
    use super::*;

    #[test]
    fn file_values_and_environment_precedence() {
        let config = parse("[jev]\nprovider = 'opencode'\nendpoint = 'file-endpoint'\nmodel = 'file-model'\napi_key = 'file-key'\n").unwrap();
        let resolve = |env: &[(&str, &str)]| {
            crate::jev::gateway_config_with_file(
                |key| {
                    env.iter()
                        .find(|(name, _)| *name == key)
                        .map(|(_, value)| value.to_string())
                },
                &config.jev,
            )
            .unwrap()
            .unwrap()
        };
        let file = resolve(&[]);
        assert_eq!(
            (&*file.endpoint, &*file.model, file.api_key.expose()),
            ("file-endpoint", "file-model", "file-key")
        );
        let env = resolve(&[
            ("JEV_PROVIDER", "vercel"),
            ("JEV_ENDPOINT", "env-endpoint"),
            ("JEV_MODEL", "env-model"),
            ("JEV_API_KEY", "env-key"),
            ("AI_GATEWAY_API_KEY", "provider-key"),
        ]);
        assert_eq!(
            (&*env.endpoint, &*env.model, env.api_key.expose()),
            ("env-endpoint", "env-model", "env-key")
        );
        for (provider, variable) in [
            ("opencode", "OPENCODE_API_KEY"),
            ("openrouter", "OPENROUTER_API_KEY"),
            ("vercel", "AI_GATEWAY_API_KEY"),
            ("typesafe", "TYPESAFE_API_KEY"),
        ] {
            assert_eq!(
                resolve(&[("JEV_PROVIDER", provider), (variable, "provider-key")])
                    .api_key
                    .expose(),
                "provider-key"
            );
        }
        assert!(crate::jev::gateway_config_with_file(
            |_| None,
            &Jev {
                provider: None,
                ..config.jev.clone()
            }
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn invalid_jev_fields_and_provider_hide_values() {
        for text in [
            "[jev]\nunknown = 'secret-value'\n",
            "[jev]\napi_key = ['secret-value']\n",
            "[jev]\nprovider = 'secret-value'\n",
        ] {
            let error = parse(text).err().unwrap().to_string();
            assert!(!error.contains("secret-value"));
        }
    }
}

#[cfg(test)]
mod images_regressions {
    use super::*;

    #[test]
    fn image_values_choose_supplied_before_file() {
        let file = Images {
            model: Some("file".into()),
            ..Images::default()
        };
        assert_eq!(
            Images::default()
                .with_fallback(file.clone())
                .model
                .as_deref(),
            Some("file")
        );
        assert_eq!(
            Images {
                model: Some("env".into()),
                ..Images::default()
            }
            .with_fallback(file)
            .model
            .as_deref(),
            Some("env")
        );
        assert!(Images::default()
            .with_fallback(Images::default())
            .model
            .is_none());
    }

    #[test]
    fn image_defaults_and_sources_resolve_without_io() {
        let values = Images::default().resolve(Images::default()).unwrap();
        assert_eq!(values.model, crate::images::DEFAULT_MODEL);
        assert_eq!(values.size, "auto");
        assert_eq!(values.quality, "auto");
        assert!(
            matches!(values.backend, ImageBackend::ChatGpt { mainline } if mainline == crate::images::CHAT_MAINLINE_DEFAULT)
        );
        let supplied = Images {
            model: Some("flag".into()),
            ..Images::default()
        };
        let environment = Images {
            model: Some("env".into()),
            quality: Some("high".into()),
            ..Images::default()
        };
        let file = Images {
            model: Some("file".into()),
            quality: Some("low".into()),
            size: Some("1024x1024".into()),
            ..Images::default()
        };
        let values = supplied.with_fallback(environment).resolve(file).unwrap();
        assert_eq!(values.model, "flag");
        assert_eq!(values.quality, "high");
        assert_eq!(values.size, "1024x1024");
        let values = Images {
            api: Some("openai".into()),
            api_key: Some("key".into()),
            ..Images::default()
        }
        .resolve(Images::default())
        .unwrap();
        assert!(
            matches!(values.backend, ImageBackend::OpenAi { base_url, .. } if base_url == crate::images::DEFAULT_BASE_URL)
        );
    }

    #[test]
    fn image_file_reads_require_missing_active_settings() {
        let common = Images {
            model: Some("model".into()),
            size: Some("auto".into()),
            quality: Some("auto".into()),
            ..Images::default()
        };
        let chatgpt = Images {
            api: Some("chatgpt".into()),
            mainline: Some("mainline".into()),
            ..common.clone()
        };
        assert!(!chatgpt.needs_file());
        let openai = Images {
            api: Some("openai".into()),
            api_key: Some("key".into()),
            base_url: Some("url".into()),
            ..common
        };
        assert!(!openai.needs_file());
        assert!(Images {
            quality: None,
            ..chatgpt
        }
        .needs_file());
        assert!(Images {
            api_key: None,
            ..openai
        }
        .needs_file());
        assert!(Images::default().needs_file());
    }

    #[test]
    fn image_auth_refresh_writes_back_to_source() {
        use crate::images::auth_source_dir;
        use std::path::Path;
        let own = Path::new("/custom");
        let legacy = Path::new("/home/test/.config/llm-stream");
        assert_eq!(auth_source_dir(own, Some(legacy), true), own);
        assert_eq!(auth_source_dir(own, Some(legacy), false), legacy);
        assert_eq!(auth_source_dir(own, None, false), own);
        assert_eq!(auth_source_dir(own, None, true), own);
    }

    #[test]
    fn image_auth_directory_precedence() {
        use std::path::{Path, PathBuf};
        let resolve = |explicit, environment, config, home| {
            crate::images::resolve_config_dir(explicit, environment, config, home)
        };
        let home = Some(Path::new("/home/test"));
        let config = Some(Path::new("/custom/config.toml"));
        assert_eq!(
            resolve(Some("~/explicit"), Some("/env"), config, home),
            (PathBuf::from("/home/test/explicit"), None)
        );
        assert_eq!(
            resolve(Some(""), Some("/env"), config, home),
            (PathBuf::from("/env"), None)
        );
        assert_eq!(
            resolve(None, Some("/env"), config, home),
            (PathBuf::from("/env"), None)
        );
        assert_eq!(
            resolve(Some(""), Some(""), config, home),
            (
                PathBuf::from("/custom"),
                Some(PathBuf::from("/home/test/.config/llm-stream"))
            )
        );
        assert_eq!(
            resolve(None, None, config, home),
            (
                PathBuf::from("/custom"),
                Some(PathBuf::from("/home/test/.config/llm-stream"))
            )
        );
        assert_eq!(
            resolve(None, None, None, home),
            (PathBuf::from("/home/test/.config/llm-stream"), None)
        );
    }

    #[test]
    fn image_file_defaults_and_invalid_values() {
        let config = parse("[images]\napi = 'openai'\napi_key = 'file-key'\nmodel = 'file-model'\nsize = '1024x1024'\nquality = 'high'\nbase_url = 'https://example.test'\nmainline = 'mainline'\n").unwrap();
        let value = toml::Value::try_from(config).unwrap();
        assert_eq!(value["images"]["model"].as_str(), Some("file-model"));
        for field in ["size", "quality", "api"] {
            let error = parse(&format!("[images]\n{field} = 'secret-value'\n"))
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains(field));
            assert!(!error.contains("secret-value"));
        }
    }
}
