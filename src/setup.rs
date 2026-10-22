//! Turns on the game's Stats API (`PacketSendRate` in `TAStatsAPI.ini`).

use std::path::{Path, PathBuf};

use crate::paths;

/// Sets `PacketSendRate` keeping the file's line endings (the game writes CRLF).
pub fn set_packet_rate(text: &str, rate: u32) -> String {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut found = false;
    let mut lines: Vec<String> = text
        .split(eol)
        .map(|l| {
            let key = l.trim_start().split('=').next().unwrap_or("").trim();
            if key.eq_ignore_ascii_case("PacketSendRate") {
                found = true;
                format!("PacketSendRate={rate}")
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        match lines
            .iter()
            .position(|l| l.trim() == "[TAGame.MatchStatsExporter_TA]")
        {
            Some(i) => lines.insert(i + 1, format!("PacketSendRate={rate}")),
            None => {
                lines.insert(0, "[TAGame.MatchStatsExporter_TA]".into());
                lines.insert(1, format!("PacketSendRate={rate}"));
            }
        }
    }
    lines.join(eol)
}

pub fn read_packet_rate(text: &str) -> Option<f64> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        k.trim()
            .eq_ignore_ascii_case("PacketSendRate")
            .then(|| v.trim().parse().ok())?
    })
}

/// Files to edit: the user config the game reads, or (if the game never ran) the defaults
/// it will be generated from.
pub fn targets(explicit: Option<&Path>) -> Vec<PathBuf> {
    if let Some(p) = explicit {
        return vec![p.to_path_buf()];
    }
    let user = paths::user_configs();
    if !user.is_empty() {
        return user;
    }
    paths::install_config_dirs()
        .into_iter()
        .map(|d| d.join("DefaultStatsAPI.ini"))
        .collect()
}

/// True if every config the game reads already has the Stats API on.
pub fn is_enabled() -> bool {
    let files = paths::user_configs();
    !files.is_empty()
        && files.iter().all(|f| {
            std::fs::read_to_string(f)
                .ok()
                .and_then(|t| read_packet_rate(&t))
                .is_some_and(|r| r > 0.0)
        })
}

pub fn run(explicit: Option<&Path>, rate: u32) -> Result<Vec<PathBuf>, String> {
    let files = targets(explicit);
    if files.is_empty() {
        return Err(
            "no Rocket League configuration found: start the game once, or pass --path".into(),
        );
    }
    for f in &files {
        let text = std::fs::read(f).map_err(|e| format!("{}: {e}", f.display()))?;
        let text = String::from_utf8_lossy(&text);
        std::fs::write(f, set_packet_rate(&text, rate))
            .map_err(|e| format!("{}: {e}", f.display()))?;
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_crlf_and_adds_missing_key() {
        let crlf = "[TAGame.MatchStatsExporter_TA]\r\nPort=49123\r\nPacketSendRate=0\r\n";
        assert_eq!(
            set_packet_rate(crlf, 10),
            "[TAGame.MatchStatsExporter_TA]\r\nPort=49123\r\nPacketSendRate=10\r\n"
        );
        let missing = "[TAGame.MatchStatsExporter_TA]\nPort=49123\n";
        assert_eq!(
            set_packet_rate(missing, 10),
            "[TAGame.MatchStatsExporter_TA]\nPacketSendRate=10\nPort=49123\n"
        );
        assert_eq!(read_packet_rate(crlf), Some(0.0));
    }
}
