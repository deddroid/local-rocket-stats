//! App settings (language, phone access), kept apart from the match data.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const APP: &str = "local-rocket-stats";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    It,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::It => "it",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code.get(..2)?.to_ascii_lowercase().as_str() {
            "en" => Some(Lang::En),
            "it" => Some(Lang::It),
            _ => None,
        }
    }

    /// The system language when supported, English otherwise.
    pub fn system() -> Self {
        sys_locale::get_locale()
            .and_then(|l| Self::from_code(&l))
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// None = follow the system language.
    pub lang: Option<Lang>,
    /// Serve the dashboard on the local network too (read-only), e.g. for a phone.
    pub lan: bool,
    pub port: u16,
    pub game_port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang: None,
            lan: false,
            port: 8765,
            game_port: 49123,
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP)
}

pub fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP)
}

pub fn log_path() -> PathBuf {
    // Windows has no "state" folder: the log goes next to the data.
    dirs::state_dir()
        .map(|d| d.join(APP))
        .unwrap_or_else(data_dir)
        .join("tracker.log")
}

fn path() -> PathBuf {
    std::env::var_os("LRS_SETTINGS")
        .map(PathBuf::from)
        .unwrap_or_else(|| config_dir().join("settings.toml"))
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(
            p,
            toml::to_string_pretty(self).map_err(std::io::Error::other)?,
        )
    }

    pub fn lang(&self) -> Lang {
        self.lang.unwrap_or_else(Lang::system)
    }
}

/// Default database path, migrating the file of the first (Python) version if present.
pub fn default_db() -> PathBuf {
    let db = data_dir().join("matches.db");
    if !db.exists() {
        if let Some(old) = dirs::data_dir().map(|d| d.join("rocket-stats").join("matches.db")) {
            if old.exists() {
                let _ = std::fs::create_dir_all(data_dir());
                if std::fs::copy(&old, &db).is_ok() {
                    // WAL mode: recent changes may still be in the side file.
                    for ext in ["-wal", "-shm"] {
                        let side = PathBuf::from(format!("{}{ext}", old.display()));
                        if side.exists() {
                            let _ = std::fs::copy(&side, format!("{}{ext}", db.display()));
                        }
                    }
                    eprintln!("Copied the previous database from {}", old.display());
                }
            }
        }
    }
    db
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_codes() {
        assert_eq!(Lang::from_code("it-IT"), Some(Lang::It));
        assert_eq!(Lang::from_code("en_US.UTF-8"), Some(Lang::En));
        assert_eq!(Lang::from_code("de"), None);
        let s: Settings = toml::from_str("lang = \"it\"\nlan = true").unwrap();
        assert_eq!((s.lang, s.lan, s.port), (Some(Lang::It), true, 8765));
    }
}
