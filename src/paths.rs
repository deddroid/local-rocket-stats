//! Where Rocket League keeps its config and logs, on Windows and under Proton on Linux.

use std::path::{Path, PathBuf};

/// The game reads its config and writes its logs in "Documents/My Games/Rocket League/TAGame".
/// Under Proton that folder lives inside the Wine prefix.
const USER_DIR: [&str; 4] = ["Documents", "My Games", "Rocket League", "TAGame"];

/// Expands a path where a `*` component matches any directory.
fn expand(root: &Path, parts: &[&str]) -> Vec<PathBuf> {
    let Some((first, rest)) = parts.split_first() else {
        return vec![root.to_path_buf()];
    };
    if *first == "*" {
        let Ok(entries) = std::fs::read_dir(root) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for e in entries.flatten() {
            if e.path().is_dir() {
                out.extend(expand(&e.path(), rest));
            }
        }
        out
    } else {
        let next = root.join(first);
        if next.exists() {
            expand(&next, rest)
        } else {
            Vec::new()
        }
    }
}

fn dedup(mut v: Vec<PathBuf>) -> Vec<PathBuf> {
    v = v
        .into_iter()
        .map(|p| p.canonicalize().unwrap_or(p))
        .collect();
    v.sort();
    v.dedup();
    v
}

/// "TAGame" folders of the user configuration (one per prefix / installation).
pub fn user_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(windows) {
        if let Some(docs) = dirs::document_dir() {
            out.extend(expand(&docs, &USER_DIR[1..]));
        }
    } else if let Some(home) = dirs::home_dir() {
        let wine_user = |prefix: &[&str]| -> Vec<String> {
            let mut p: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
            p.extend(["drive_c", "users", "*"].map(String::from));
            p.extend(USER_DIR.map(String::from));
            p
        };
        let roots: [(&str, &[&str]); 5] = [
            (
                ".local/share/Steam/steamapps/compatdata",
                &["252950", "pfx"],
            ),
            (".steam/steam/steamapps/compatdata", &["252950", "pfx"]),
            (
                ".var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/compatdata",
                &["252950", "pfx"],
            ),
            ("Games/Heroic/Prefixes", &["*", "pfx"]),
            ("Games/Heroic/Prefixes", &["*"]),
        ];
        for (root, prefix) in roots {
            let parts = wine_user(prefix);
            out.extend(expand(
                &home.join(root),
                &parts.iter().map(String::as_str).collect::<Vec<_>>(),
            ));
        }
    }
    dedup(out)
}

/// `TAStatsAPI.ini` files the game actually reads.
pub fn user_configs() -> Vec<PathBuf> {
    user_dirs()
        .into_iter()
        .map(|d| d.join("Config").join("TAStatsAPI.ini"))
        .filter(|p| p.is_file())
        .collect()
}

pub fn log_dirs() -> Vec<PathBuf> {
    user_dirs()
        .into_iter()
        .map(|d| d.join("Logs"))
        .filter(|p| p.is_dir())
        .collect()
}

/// `TAGame/Config` folders of game installations (holding `DefaultStatsAPI.ini`).
pub fn install_config_dirs() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for base in [
            "C:\\Program Files\\Epic Games",
            "C:\\Program Files (x86)\\Steam\\steamapps\\common",
            "C:\\Program Files\\Steam\\steamapps\\common",
        ] {
            roots.push(PathBuf::from(base));
        }
    } else if let Some(home) = dirs::home_dir() {
        for rel in [
            "Games",
            ".local/share/Steam/steamapps/common",
            ".steam/steam/steamapps/common",
            ".var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/common",
            ".var/app/com.heroicgameslauncher.hgl/config/heroic",
        ] {
            roots.push(home.join(rel));
        }
    }
    let mut out = Vec::new();
    for root in roots {
        for depth in 1..=3 {
            let mut parts = vec!["*"; depth];
            parts.extend(["TAGame", "Config"]);
            out.extend(
                expand(&root, &parts)
                    .into_iter()
                    .filter(|d| d.join("DefaultStatsAPI.ini").is_file()),
            );
        }
    }
    dedup(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_matches_wildcards() {
        let tmp = std::env::temp_dir().join(format!("lrs-paths-{}", std::process::id()));
        let target =
            tmp.join("252950/pfx/drive_c/users/steamuser/Documents/My Games/Rocket League/TAGame");
        std::fs::create_dir_all(&target).unwrap();
        let found = expand(
            &tmp,
            &[
                "252950",
                "pfx",
                "drive_c",
                "users",
                "*",
                "Documents",
                "My Games",
                "Rocket League",
                "TAGame",
            ],
        );
        assert_eq!(found, vec![target]);
        std::fs::remove_dir_all(tmp).unwrap();
    }
}
