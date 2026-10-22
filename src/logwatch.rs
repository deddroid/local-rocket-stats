//! Reads MMR and ranked match summaries from the game log (`Launch.log`).
//!
//! When the player joins a queue the game writes, for example:
//!
//! ```text
//! [0940.57] Matchmaking: OnlineGameMatchmaking_X::RecordStart
//! [0940.57] Matchmaking: Pre-divide PartyLeaderMMR: 63.2873
//! [0940.57] Matchmaking: PartyLeaderTier=(18)
//! [0940.57] Matchmaking: PreferredRegions.Length=(5) PreferredPlaylists.Length=(1) Party.GetOrderedPartyMemberIDs().Length=(1)
//! [0946.99] Party: HandleServerReserved (Reservation=(ServerName="...",Playlist=11,...,ReservationID="CA50..."
//! ```
//!
//! The value is the internal skill (mu): the MMR shown in game is `mu * 20 + 100`. It is the MMR
//! *before* the match, so it is also the MMR *after* the previous match in the same playlist.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Local, NaiveDate, TimeZone};

use crate::collector::Collector;
use crate::db::{Database, MatchRecord};
use crate::gamedata;
use crate::logln;

pub fn mu_to_mmr(mu: f64) -> f64 {
    (mu * 20.0 + 100.0).round()
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)? + start.len();
    let j = s[i..].find(end)? + i;
    Some(&s[i..j])
}

/// Date the log was opened. Its format follows the system language (dd/mm or mm/dd): pick the
/// valid reading closest to (and not after) the file's last modification.
pub fn parse_open_time(line: &str, file_mtime: SystemTime) -> Option<DateTime<Local>> {
    let rest = line.split("Log file open, ").nth(1)?.trim();
    let (date, time) = rest.split_once(' ')?;
    let d: Vec<i32> = date.split('/').filter_map(|x| x.parse().ok()).collect();
    let t: Vec<u32> = time.split(':').filter_map(|x| x.parse().ok()).collect();
    if d.len() != 3 || t.len() != 3 {
        return None;
    }
    let year = if d[2] < 100 { d[2] + 2000 } else { d[2] };
    let mtime: DateTime<Local> = file_mtime.into();
    let mut candidates: Vec<DateTime<Local>> = [(d[0], d[1]), (d[1], d[0])]
        .iter()
        .filter_map(|&(day, month)| {
            let nd = NaiveDate::from_ymd_opt(year, month as u32, day as u32)?;
            Local
                .from_local_datetime(&nd.and_hms_opt(t[0], t[1], t[2])?)
                .single()
        })
        .collect();
    let valid: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|c| *c <= mtime + chrono::Duration::minutes(5))
        .collect();
    if !valid.is_empty() {
        candidates = valid;
    }
    candidates
        .into_iter()
        .min_by_key(|c| (mtime - *c).num_seconds().abs())
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueueSample {
    pub playlist: i64,
    pub mmr: f64,
    pub tier: Option<i64>,
    pub at: DateTime<Local>,
}

#[derive(Default)]
struct Pending {
    at: Option<DateTime<Local>>,
    mu: Option<f64>,
    tier: Option<i64>,
    solo: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct LogMatch {
    pub reservation: String,
    pub playlist: i64,
    pub start: DateTime<Local>,
    pub end: Option<DateTime<Local>>,
    pub arena: Option<String>,
    loaded: bool,
    /// Always "your team - opponents" in the log.
    pub score: Option<(i64, i64)>,
    pub overtime: bool,
    pub points: Option<i64>,
    pub me_id: Option<String>,
    pub abandoned: bool,
}

pub enum LogEvent {
    Sample(QueueSample),
    Match(LogMatch),
}

/// State machine over the lines of a single log file.
pub struct LogParser {
    file_mtime: SystemTime,
    opened: Option<DateTime<Local>>,
    pending: Option<Pending>,
    current: Option<LogMatch>,
    seen: HashSet<String>,
}

impl LogParser {
    pub fn new(file_mtime: SystemTime) -> Self {
        Self {
            file_mtime,
            opened: None,
            pending: None,
            current: None,
            seen: HashSet::new(),
        }
    }

    pub fn feed(&mut self, line: &str, out: &mut Vec<LogEvent>) {
        let Some(opened) = self.opened else {
            if line.contains("Log file open, ") {
                self.opened = parse_open_time(line, self.file_mtime);
            }
            return;
        };
        let Some(secs) = between(line, "[", "]")
            .filter(|_| line.starts_with('['))
            .and_then(|s| s.parse::<f64>().ok())
        else {
            return;
        };
        let when = opened + chrono::Duration::milliseconds((secs * 1000.0) as i64);

        if line.contains("Matchmaking: OnlineGameMatchmaking_X::RecordStart") {
            self.pending = Some(Pending {
                at: Some(when),
                ..Default::default()
            });
            return;
        }
        if line.contains("HandleServerReserved (Reservation=(") {
            let playlist = between(line, "Playlist=", ",").and_then(|s| s.parse().ok());
            let reservation = between(line, "ReservationID=\"", "\"");
            if let (Some(playlist), Some(reservation)) = (playlist, reservation) {
                self.on_reserved(playlist, reservation, when, out);
            }
            return;
        }
        if let Some(m) = &mut self.current {
            if let Some(rest) = line.split("Log: LoadMap: ").nth(1) {
                let token = rest.split(['?', ' ']).next().unwrap_or("");
                match token.split_once('/') {
                    Some((server, arena))
                        if server.contains(':')
                            && server
                                .chars()
                                .all(|c| c.is_ascii_digit() || c == '.' || c == ':') =>
                    {
                        m.arena = Some(arena.to_string());
                        m.loaded = true;
                    }
                    // Back to the menu/training without the end of the match: abandoned.
                    _ if m.loaded => self.finish(when, true, out),
                    _ => {}
                }
                return;
            }
            if line.contains("Set rich presence to: ") {
                if let Some(pl) = line
                    .split(" data: Playlist-")
                    .nth(1)
                    .and_then(|s| s.trim().parse::<i64>().ok())
                {
                    if pl == m.playlist {
                        let head = line.split(" data: Playlist-").next().unwrap_or("");
                        if let Some(open) = head.rfind(" (") {
                            let score = head[open + 2..].trim_end_matches(')');
                            if let Some((a, b)) = score.split_once(" - ") {
                                if let (Ok(a), Ok(b)) = (a.trim().parse(), b.trim().parse()) {
                                    m.score = Some((a, b));
                                }
                            }
                            m.overtime |= head[..open]
                                .rsplit(' ')
                                .next()
                                .is_some_and(|t| t.starts_with('+'));
                        }
                    }
                }
                return;
            }
            if let Some(points) = between(line, "Current player match score = ", ",") {
                m.points = points.trim().parse().ok();
                m.me_id = between(line, "UniqueId=(", ")").map(str::to_string);
                return;
            }
            if line.contains("Party: Ranked Game Finished") {
                self.finish(when, false, out);
                return;
            }
        }
        let Some(p) = &mut self.pending else { return };
        if let Some(v) = line
            .split("Matchmaking: Pre-divide PartyLeaderMMR: ")
            .nth(1)
        {
            p.mu = v.trim().parse().ok();
        } else if let Some(v) = between(line, "Matchmaking: PartyLeaderTier=(", ")") {
            p.tier = v.parse().ok();
        } else if let (Some(a), Some(b)) = (
            between(line, "PreferredPlaylists.Length=(", ")"),
            between(line, "GetOrderedPartyMemberIDs().Length=(", ")"),
        ) {
            // In a party the value is the party leader's; with several playlists we can't tell which.
            p.solo = Some(a == "1" && b == "1");
        }
    }

    fn on_reserved(
        &mut self,
        playlist: i64,
        reservation: &str,
        when: DateTime<Local>,
        out: &mut Vec<LogEvent>,
    ) {
        if !self.seen.insert(reservation.to_string()) {
            return; // the line appears twice
        }
        if let Some(Pending {
            at: Some(at),
            mu: Some(mu),
            tier,
            solo: Some(true),
        }) = self.pending.take()
        {
            out.push(LogEvent::Sample(QueueSample {
                playlist,
                mmr: mu_to_mmr(mu),
                tier,
                at,
            }));
        }
        if self.current.as_ref().is_some_and(|m| m.loaded) {
            self.finish(when, true, out);
        }
        self.current = gamedata::is_ranked(Some(playlist)).then(|| LogMatch {
            reservation: reservation.to_string(),
            playlist,
            start: when,
            end: None,
            arena: None,
            loaded: false,
            score: None,
            overtime: false,
            points: None,
            me_id: None,
            abandoned: false,
        });
    }

    fn finish(&mut self, when: DateTime<Local>, abandoned: bool, out: &mut Vec<LogEvent>) {
        if let Some(mut m) = self.current.take().filter(|m| m.loaded) {
            m.end = Some(when);
            m.abandoned = abandoned;
            out.push(LogEvent::Match(m));
        }
    }
}

impl LogMatch {
    pub fn to_record(&self) -> MatchRecord {
        let (a, b) = self.score.unzip();
        let result = if self.abandoned {
            "A"
        } else {
            match self.score {
                Some((a, b)) if a > b => "W",
                Some((a, b)) if a < b => "L",
                _ => "?",
            }
        };
        let iso = |t: DateTime<Local>| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
        MatchRecord {
            guid: format!("log-{}", self.reservation),
            started_at: iso(self.start),
            ended_at: self.end.map(iso),
            playlist_id: Some(self.playlist),
            arena: self.arena.clone(),
            my_team: Some(0),
            team0_score: a,
            team1_score: b,
            result: result.into(),
            overtime: self.overtime,
            me_id: self.me_id.clone(),
            score: self.points,
            ..Default::default()
        }
    }
}

/// Applies parsed events to the database. Returns true if anything changed.
fn apply(db: &Database, events: Vec<LogEvent>) -> bool {
    let mut changed = false;
    for ev in events {
        match ev {
            LogEvent::Sample(s) => {
                let at = s.at.to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
                if db
                    .add_mmr_sample(s.playlist, s.mmr, s.tier, &at)
                    .unwrap_or(false)
                {
                    let tier = s.tier.map_or("?".into(), |t| t.to_string());
                    logln!(
                        "MMR from the log: playlist {} -> {} (tier {tier})",
                        s.playlist,
                        s.mmr
                    );
                    changed = true;
                }
            }
            LogEvent::Match(m) => {
                if db.add_log_match(&m.to_record()).unwrap_or(false) {
                    let score = m.score.map_or("?".into(), |(a, b)| format!("{a}-{b}"));
                    logln!(
                        "Match recovered from the log: playlist {} {score}",
                        m.playlist
                    );
                    changed = true;
                }
            }
        }
    }
    if changed {
        let _ = db.reconcile_mmr();
    }
    changed
}

fn parse_file(path: &Path) -> std::io::Result<Vec<LogEvent>> {
    let mtime = fs::metadata(path)?.modified()?;
    let mut parser = LogParser::new(mtime);
    let text = String::from_utf8_lossy(&fs::read(path)?).into_owned();
    let mut out = Vec::new();
    for line in text.lines() {
        parser.feed(line.trim_end_matches('\r'), &mut out);
    }
    Ok(out)
}

struct Tail {
    id: (u64, SystemTime),
    pos: u64,
    parser: LogParser,
    rest: Vec<u8>,
}

/// Identity of a file across restarts of the game (it is recreated, and the old one renamed).
fn file_id(meta: &fs::Metadata) -> (u64, SystemTime) {
    #[cfg(unix)]
    let ino = std::os::unix::fs::MetadataExt::ino(meta);
    #[cfg(not(unix))]
    let ino = 0;
    (ino, meta.created().unwrap_or(SystemTime::UNIX_EPOCH))
}

fn tail(path: &Path, state: Option<Tail>, out: &mut Vec<LogEvent>) -> std::io::Result<Tail> {
    let meta = fs::metadata(path)?;
    let id = file_id(&meta);
    let mut t = match state {
        Some(t) if t.id == id && meta.len() >= t.pos => t,
        // New game launch (or truncated file): start over.
        _ => Tail {
            id,
            pos: 0,
            parser: LogParser::new(meta.modified()?),
            rest: Vec::new(),
        },
    };
    if meta.len() == t.pos {
        return Ok(t);
    }
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(t.pos))?;
    let mut data = Vec::new();
    f.read_to_end(&mut data)?;
    t.pos += data.len() as u64;
    t.rest.extend(data);
    // Keep the last, possibly incomplete, line for the next round.
    let cut = t
        .rest
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    let complete: Vec<u8> = t.rest.drain(..cut).collect();
    for line in String::from_utf8_lossy(&complete).lines() {
        t.parser.feed(line.trim_end_matches('\r'), out);
    }
    Ok(t)
}

/// Reads old logs once, then follows `Launch.log` while the app runs.
pub fn spawn(db: Arc<Database>, collector: Arc<Collector>, log_dirs: Vec<PathBuf>) {
    thread::Builder::new()
        .name("logwatch".into())
        .spawn(move || {
            if log_dirs.is_empty() {
                logln!("Game log folder not found: MMR can only be typed by hand");
                return;
            }
            logln!(
                "Reading MMR from the game logs in: {}",
                log_dirs
                    .iter()
                    .map(|d| d.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            // The game keeps a few previous logs as backups.
            let mut backups: Vec<PathBuf> = log_dirs
                .iter()
                .filter_map(|d| fs::read_dir(d).ok())
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("Launch-backup-") && n.ends_with(".log"))
                })
                .collect();
            backups.sort();
            let mut events = Vec::new();
            for p in backups {
                if let Ok(ev) = parse_file(&p) {
                    events.extend(ev);
                }
            }
            if apply(&db, events) {
                collector.bump();
            }
            let mut tails: Vec<(PathBuf, Option<Tail>)> = log_dirs
                .iter()
                .map(|d| (d.join("Launch.log"), None))
                .collect();
            loop {
                let mut events = Vec::new();
                for (path, state) in &mut tails {
                    *state = tail(path, state.take(), &mut events).ok();
                }
                if apply(&db, events) {
                    collector.bump();
                }
                thread::sleep(Duration::from_secs(1));
            }
        })
        .expect("logwatch thread");
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = r#"Log: Log file open, 21/10/2026 22:16:24
Init: something
[0789.48] Matchmaking: OnlineGameMatchmaking_X::RecordStart
[0789.48] Matchmaking: Pre-divide PartyLeaderMMR: 62.8112
[0789.48] Matchmaking: Post-divide PartyLeaderMMR: 62.8112
[0789.48] Matchmaking: PartyLeaderTier=(18)
[0789.48] Matchmaking: PreferredRegions.Length=(5) PreferredPlaylists.Length=(1) Party.GetOrderedPartyMemberIDs().Length=(1)
[0799.09] Party: HandleServerReserved (Reservation=(ServerName="EU5-62a12b43-Apex",Playlist=11,Region="EU",ReservationID="906F1F0F3A314FF9B1AEF737F312A98A",DSRToken="x"
[0799.10] Party: HandleServerReserved (Reservation=(ServerName="EU5-62a12b43-Apex",Playlist=11,Region="EU",ReservationID="906F1F0F3A314FF9B1AEF737F312A98A",DSRToken="x"
[0803.32] Log: LoadMap: JoinGameTransition?Name=Me?game=TAGame.GameInfo_Transition_TA
[0804.57] Log: LoadMap: 18.157.43.51:9013/NeoTokyo_Arcade_P?Name=Me?game=TAGame.GameInfo_Soccar_TA
[0845.73] DevOnline: Set rich presence to: Doppio su Neo Tokyo (Arcade) 4:39 (1 - 0) data: Playlist-11
[1692.11] DevOnline: Set rich presence to: Doppio su Neo Tokyo (Arcade) +3:22 (4 - 3) data: Playlist-11
[1694.87] XPProgression: GFxHUD_TA.HandleGameStateChanged Current player match score = 961, UniqueId=(Steam|76561198049879898|0), with total match time = 505.7897 seconds
[1694.87] Party: Ranked Game Finished, Clear party member server
[1697.86] Party: Ranked Game Finished, Clear party member server
[1701.00] Matchmaking: OnlineGameMatchmaking_X::RecordStart
[1701.00] Matchmaking: Pre-divide PartyLeaderMMR: 70.0
[1701.00] Matchmaking: PreferredRegions.Length=(5) PreferredPlaylists.Length=(1) Party.GetOrderedPartyMemberIDs().Length=(2)
[1715.67] Party: HandleServerReserved (Reservation=(ServerName="EU5-x",Playlist=11,Region="EU",ReservationID="FF87",DSRToken="x"
[1720.78] Log: LoadMap: 18.156.222.186:9054/cs_day_p?Name=Me?game=TAGame.GameInfo_Soccar_TA
[1760.86] DevOnline: Set rich presence to: Doppio su Champions Field (Giorno) 4:38 (0 - 1) data: Playlist-11
[1800.00] Log: LoadMap: MENU_Main_p?closed
"#;

    #[test]
    fn parses_samples_matches_party_and_abandon() {
        let mtime = SystemTime::now();
        let mut p = LogParser::new(mtime);
        let mut out = Vec::new();
        for line in LOG.lines() {
            p.feed(line, &mut out);
        }
        let samples: Vec<_> = out
            .iter()
            .filter_map(|e| {
                if let LogEvent::Sample(s) = e {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();
        let matches: Vec<_> = out
            .iter()
            .filter_map(|e| {
                if let LogEvent::Match(m) = e {
                    Some(m)
                } else {
                    None
                }
            })
            .collect();
        // The party queue (Length=(2)) is skipped.
        assert_eq!(samples.len(), 1);
        assert_eq!(
            (samples[0].playlist, samples[0].mmr, samples[0].tier),
            (11, 1356.0, Some(18))
        );
        assert_eq!(matches.len(), 2);
        let m = &matches[0];
        assert_eq!(
            (m.score, m.overtime, m.points, m.arena.as_deref()),
            (Some((4, 3)), true, Some(961), Some("NeoTokyo_Arcade_P"))
        );
        assert_eq!(m.to_record().result, "W");
        assert!(matches[1].abandoned);
        assert_eq!(matches[1].to_record().result, "A");
    }

    #[test]
    fn open_time_picks_plausible_day_month_order() {
        // 03/04 could be 3 April or 4 March: the file was modified in April.
        let mtime: SystemTime = Local.with_ymd_and_hms(2026, 4, 3, 23, 0, 0).unwrap().into();
        let t = parse_open_time("Log: Log file open, 03/04/2026 20:00:00", mtime).unwrap();
        assert_eq!(t.format("%Y-%m-%d").to_string(), "2026-04-03");
    }
}
