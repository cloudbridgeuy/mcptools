use crate::prelude::{println, *};
use mcptools_core::config::AppConfig;
use std::path::{Path, PathBuf};

#[derive(Debug, clap::Parser)]
pub struct App {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    Check,
    Path,
}

pub fn path() -> Result<PathBuf> {
    Ok(mcptools_core::config::config_path(
        std::env::var_os("MCPTOOLS_CONFIG").as_deref(),
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        std::env::var_os("HOME").as_deref().map(Path::new),
        &std::env::current_dir()?,
    )?)
}

pub fn load(path: &Path) -> Result<Option<AppConfig>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(eyre!("Cannot read config {}: {error}", path.display())),
    };
    Ok(Some(mcptools_core::config::parse(&text).map_err(
        |error| eyre!("Invalid config {}: {error}", path.display()),
    )?))
}

pub fn atlassian() -> Result<mcptools_core::config::Atlassian> {
    Ok(optional()?.unwrap_or_default().atlassian)
}

pub fn linear() -> Result<mcptools_core::config::Linear> {
    Ok(optional()?.unwrap_or_default().linear)
}

fn optional() -> Result<Option<AppConfig>> {
    let Some(path) = mcptools_core::config::optional_config_path(
        std::env::var_os("MCPTOOLS_CONFIG").as_deref(),
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        std::env::var_os("HOME").as_deref().map(Path::new),
        &std::env::current_dir()?,
    ) else {
        return Ok(None);
    };
    load(&path)
}

pub fn run(app: App) -> Result<()> {
    let path = path()?;
    match app.command {
        Commands::Path => println!("{}", path.display()),
        Commands::Check => match load(&path)? {
            Some(_) => {
                mcptools_core::config::cache_path(&path)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if std::fs::metadata(&path)?.permissions().mode() & 0o044 != 0 {
                        std::eprintln!(
                            "warning: config is group or world readable; run chmod 600 {}",
                            path.display()
                        );
                    }
                }
                println!("ok: {}", path.display());
            }
            None => println!("no config at {}; defaults apply", path.display()),
        },
    }
    Ok(())
}
