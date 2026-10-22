//! SQLite store for matches, MMR samples read from the game log, and settings.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Duration, FixedOffset};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use serde_json::{Map, Value, json};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS matches (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    guid        TEXT UNIQUE NOT NULL,
    started_at  TEXT NOT NULL,
    ended_at    TEXT,
    playlist_id INTEGER,
    arena       TEXT,
    my_team     INTEGER,
    team0_score INTEGER,
    team1_score INTEGER,
    winner_team INTEGER,
    result      TEXT,          -- W, L, A (abandoned), ? (unknown)
    overtime    INTEGER DEFAULT 0,
    me_id       TEXT,
    me_name     TEXT,
    score       INTEGER,
    goals       INTEGER,
    assists     INTEGER,
    saves       INTEGER,
    shots       INTEGER,
    demos       INTEGER,
    touches     INTEGER,
    mmr         REAL,          -- MMR after the match
    players     TEXT,          -- JSON: every player and their stats
    goals_log   TEXT,          -- JSON: list of goals
    statfeed    TEXT,          -- JSON: statfeed event counts for the player
    notes       TEXT
);
CREATE INDEX IF NOT EXISTS idx_matches_started ON matches(started_at);
CREATE TABLE IF NOT EXISTS mmr_samples (
    playlist_id INTEGER NOT NULL,
    at          TEXT NOT NULL,     -- when the player joined the queue
    mmr         REAL NOT NULL,     -- MMR before the match
    tier        INTEGER,
    PRIMARY KEY (playlist_id, at)
);
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT
);
";

/// Queue -> match start: the MMR read in the queue belongs to the match that follows.
pub const BEFORE_WINDOW_MIN: i64 = 30;
/// Match end -> next queue: older samples are not trusted as "MMR after".
pub const AFTER_WINDOW_DAYS: i64 = 14;

pub fn parse_time(s: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(s).ok()
}

/// A match as saved by the Stats API collector or rebuilt from the game log.
#[derive(Clone, Debug, Default, Serialize)]
pub struct MatchRecord {
    pub guid: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub playlist_id: Option<i64>,
    pub arena: Option<String>,
    pub my_team: Option<i64>,
    pub team0_score: Option<i64>,
    pub team1_score: Option<i64>,
    pub winner_team: Option<i64>,
    pub result: String,
    pub overtime: bool,
    pub me_id: Option<String>,
    pub me_name: Option<String>,
    pub score: Option<i64>,
    pub goals: Option<i64>,
    pub assists: Option<i64>,
    pub saves: Option<i64>,
    pub shots: Option<i64>,
    pub demos: Option<i64>,
    pub touches: Option<i64>,
    pub players: Option<Value>,
    pub goals_log: Option<Value>,
    pub statfeed: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct Sample {
    pub at: DateTime<FixedOffset>,
    pub mmr: f64,
    pub tier: Option<i64>,
}

pub struct Database {
    conn: Mutex<Connection>,
}

const JSON_COLUMNS: [&str; 3] = ["players", "goals_log", "statfeed"];

impl Database {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(SCHEMA)?;
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(matches)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<Result<_, _>>()?;
        // Migrations from earlier versions.
        if !cols.iter().any(|c| c == "mmr_auto") {
            // 1 = MMR read from the game log, 0 = typed by the user.
            conn.execute(
                "ALTER TABLE matches ADD COLUMN mmr_auto INTEGER DEFAULT 0",
                [],
            )?;
        }
        if !cols.iter().any(|c| c == "source") {
            // api = live from the Stats API, log = rebuilt from the game log.
            conn.execute(
                "ALTER TABLE matches ADD COLUMN source TEXT DEFAULT 'api'",
                [],
            )?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // --- matches -----------------------------------------------------------------------------

    /// Inserts or updates a live match (by guid). Never touches mmr/notes.
    pub fn upsert_match(&self, m: &MatchRecord) -> rusqlite::Result<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO matches (guid, started_at, ended_at, playlist_id, arena, my_team,
                team0_score, team1_score, winner_team, result, overtime, me_id, me_name, score,
                goals, assists, saves, shots, demos, touches, players, goals_log, statfeed, source)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,'api')
             ON CONFLICT(guid) DO UPDATE SET started_at=excluded.started_at, ended_at=excluded.ended_at,
                playlist_id=excluded.playlist_id, arena=excluded.arena, my_team=excluded.my_team,
                team0_score=excluded.team0_score, team1_score=excluded.team1_score,
                winner_team=excluded.winner_team, result=excluded.result, overtime=excluded.overtime,
                me_id=excluded.me_id, me_name=excluded.me_name, score=excluded.score,
                goals=excluded.goals, assists=excluded.assists, saves=excluded.saves,
                shots=excluded.shots, demos=excluded.demos, touches=excluded.touches,
                players=excluded.players, goals_log=excluded.goals_log, statfeed=excluded.statfeed",
            params![
                m.guid, m.started_at, m.ended_at, m.playlist_id, m.arena, m.my_team, m.team0_score,
                m.team1_score, m.winner_team, m.result, m.overtime, m.me_id, m.me_name, m.score,
                m.goals, m.assists, m.saves, m.shots, m.demos, m.touches,
                m.players.as_ref().map(Value::to_string),
                m.goals_log.as_ref().map(Value::to_string),
                m.statfeed.as_ref().map(Value::to_string),
            ],
        )?;
        // The same match rebuilt from the log has fewer details: the live one wins.
        for id in overlapping(&conn, m, Some("log"))? {
            conn.execute("DELETE FROM matches WHERE id=?1", [id])?;
        }
        conn.query_row("SELECT id FROM matches WHERE guid=?1", [&m.guid], |r| {
            r.get(0)
        })
    }

    /// Adds a match rebuilt from the game log unless it is already known. True if new.
    pub fn add_log_match(&self, m: &MatchRecord) -> rusqlite::Result<bool> {
        let conn = self.lock();
        let exists: Option<i64> = conn
            .query_row("SELECT 1 FROM matches WHERE guid=?1", [&m.guid], |r| {
                r.get(0)
            })
            .optional()?;
        if exists.is_some() || !overlapping(&conn, m, None)?.is_empty() {
            return Ok(false);
        }
        conn.execute(
            "INSERT INTO matches (guid, started_at, ended_at, playlist_id, arena, my_team,
                team0_score, team1_score, result, overtime, me_id, score, source)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'log')",
            params![
                m.guid,
                m.started_at,
                m.ended_at,
                m.playlist_id,
                m.arena,
                m.my_team,
                m.team0_score,
                m.team1_score,
                m.result,
                m.overtime,
                m.me_id,
                m.score
            ],
        )?;
        Ok(true)
    }

    /// Every match as JSON for the dashboard, with the MMR read in the queue before it.
    pub fn list_matches(&self) -> rusqlite::Result<Vec<Value>> {
        let conn = self.lock();
        let samples = samples_by_playlist(&conn)?;
        let mut stmt = conn.prepare("SELECT * FROM matches ORDER BY started_at, id")?;
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt.query_map([], |r| row_to_json(r, &names))?;
        let mut out = Vec::new();
        for row in rows {
            let mut m = row?;
            let (mut before, mut tier) = (Value::Null, Value::Null);
            let pl = m.get("playlist_id").and_then(Value::as_i64);
            let start = m
                .get("started_at")
                .and_then(Value::as_str)
                .and_then(parse_time);
            if let (Some(sl), Some(start)) = (pl.and_then(|p| samples.get(&p)), start) {
                if let Some(s) = sl.iter().rev().find(|s| s.at <= start) {
                    if start - s.at <= Duration::minutes(BEFORE_WINDOW_MIN) {
                        before = json!(s.mmr);
                        tier = json!(s.tier);
                    }
                }
            }
            m.insert("mmr_before".into(), before);
            m.insert("tier".into(), tier);
            out.push(Value::Object(m));
        }
        Ok(out)
    }

    /// Edits the user-editable fields. A typed MMR is never overwritten by the log.
    pub fn update_match(
        &self,
        id: i64,
        mmr: Option<Option<f64>>,
        notes: Option<Option<String>>,
        result: Option<String>,
    ) -> rusqlite::Result<bool> {
        let conn = self.lock();
        let mut changed = 0;
        if let Some(mmr) = mmr {
            changed += conn.execute(
                "UPDATE matches SET mmr=?1, mmr_auto=0 WHERE id=?2",
                params![mmr, id],
            )?;
        }
        if let Some(notes) = notes {
            changed += conn.execute(
                "UPDATE matches SET notes=?1 WHERE id=?2",
                params![notes, id],
            )?;
        }
        if let Some(result) = result {
            changed += conn.execute(
                "UPDATE matches SET result=?1 WHERE id=?2",
                params![result, id],
            )?;
        }
        Ok(changed > 0)
    }

    pub fn delete_match(&self, id: i64) -> rusqlite::Result<bool> {
        Ok(self
            .lock()
            .execute("DELETE FROM matches WHERE id=?1", [id])?
            > 0)
    }

    // --- MMR from the game log ----------------------------------------------------------------

    pub fn add_mmr_sample(
        &self,
        playlist: i64,
        mmr: f64,
        tier: Option<i64>,
        at: &str,
    ) -> rusqlite::Result<bool> {
        Ok(self.lock().execute(
            "INSERT OR IGNORE INTO mmr_samples (playlist_id, at, mmr, tier) VALUES (?1, ?2, ?3, ?4)",
            params![playlist, at, mmr, tier],
        )? > 0)
    }

    /// Latest MMR/tier per playlist (from the most recent queue).
    pub fn latest_ranks(&self) -> rusqlite::Result<Value> {
        let conn = self.lock();
        let mut out = Map::new();
        for (pl, sl) in samples_by_playlist(&conn)? {
            if let Some(s) = sl.last() {
                out.insert(
                    pl.to_string(),
                    json!({"mmr": s.mmr, "tier": s.tier, "at": s.at.to_rfc3339()}),
                );
            }
        }
        Ok(Value::Object(out))
    }

    /// Gives every match its final MMR: the one read at the first following queue in the same
    /// playlist. Typed values are left alone. Returns the number of changes.
    pub fn reconcile_mmr(&self) -> rusqlite::Result<usize> {
        let conn = self.lock();
        let samples = samples_by_playlist(&conn)?;
        let mut stmt = conn.prepare(
            "SELECT id, playlist_id, started_at, ended_at, mmr, mmr_auto, source, result
             FROM matches WHERE ended_at IS NOT NULL ORDER BY started_at",
        )?;
        type Row = (
            i64,
            Option<i64>,
            String,
            String,
            Option<f64>,
            Option<i64>,
            Option<String>,
            Option<String>,
        );
        let rows: Vec<Row> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            })?
            .collect::<Result<_, _>>()?;
        drop(stmt);
        let mut changed = 0;
        for (id, pl, started, ended, mmr, auto, source, result) in rows {
            let (Some(sl), Some(start), Some(end)) = (
                pl.and_then(|p| samples.get(&p)),
                parse_time(&started),
                parse_time(&ended),
            ) else {
                continue;
            };
            let after = sl
                .iter()
                .find(|s| s.at > end)
                .filter(|s| s.at - end <= Duration::days(AFTER_WINDOW_DAYS));
            // Log matches: win/loss from the MMR change is more reliable than the score (forfeits).
            if source.as_deref() == Some("log") && result.as_deref() != Some("A") {
                let before = sl
                    .iter()
                    .rev()
                    .find(|s| s.at <= start)
                    .filter(|s| start - s.at <= Duration::minutes(BEFORE_WINDOW_MIN));
                if let (Some(b), Some(a)) = (before, after) {
                    let r = if a.mmr > b.mmr {
                        "W"
                    } else if a.mmr < b.mmr {
                        "L"
                    } else {
                        result.as_deref().unwrap_or("?")
                    };
                    if Some(r) != result.as_deref() {
                        changed += conn
                            .execute("UPDATE matches SET result=?1 WHERE id=?2", params![r, id])?;
                    }
                }
            }
            if mmr.is_some() && auto != Some(1) {
                continue;
            }
            if let Some(a) = after {
                if mmr != Some(a.mmr) {
                    changed += conn.execute(
                        "UPDATE matches SET mmr=?1, mmr_auto=1 WHERE id=?2",
                        params![a.mmr, id],
                    )?;
                }
            }
        }
        Ok(changed)
    }

    // --- settings -----------------------------------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Option<Value> {
        let conn = self.lock();
        let raw: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten();
        raw.and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn set_setting(&self, key: &str, value: &Value) -> rusqlite::Result<()> {
        self.lock().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value.to_string()],
        )?;
        Ok(())
    }

    pub fn me_ids(&self) -> Vec<String> {
        self.get_setting("me_ids")
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }
}

fn row_to_json(r: &Row, names: &[String]) -> rusqlite::Result<Map<String, Value>> {
    use rusqlite::types::ValueRef;
    let mut m = Map::new();
    for (i, name) in names.iter().enumerate() {
        let v = match r.get_ref(i)? {
            ValueRef::Null => Value::Null,
            ValueRef::Integer(n) => json!(n),
            ValueRef::Real(f) => json!(f),
            ValueRef::Text(t) => {
                let s = String::from_utf8_lossy(t).into_owned();
                if JSON_COLUMNS.contains(&name.as_str()) {
                    serde_json::from_str(&s).unwrap_or(Value::Null)
                } else {
                    Value::String(s)
                }
            }
            ValueRef::Blob(_) => Value::Null,
        };
        m.insert(name.clone(), v);
    }
    Ok(m)
}

fn samples_by_playlist(conn: &Connection) -> rusqlite::Result<HashMap<i64, Vec<Sample>>> {
    let mut out: HashMap<i64, Vec<Sample>> = HashMap::new();
    let mut stmt = conn.prepare("SELECT playlist_id, at, mmr, tier FROM mmr_samples")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, f64>(2)?,
            r.get::<_, Option<i64>>(3)?,
        ))
    })?;
    for row in rows {
        let (pl, at, mmr, tier) = row?;
        if let Some(at) = parse_time(&at) {
            out.entry(pl).or_default().push(Sample { at, mmr, tier });
        }
    }
    for sl in out.values_mut() {
        sl.sort_by_key(|s| s.at);
    }
    Ok(out)
}

/// Ids of matches in the same playlist whose time span overlaps `m`.
fn overlapping(
    conn: &Connection,
    m: &MatchRecord,
    source: Option<&str>,
) -> rusqlite::Result<Vec<i64>> {
    let (Some(start), end) = (
        parse_time(&m.started_at),
        m.ended_at.as_deref().and_then(parse_time),
    ) else {
        return Ok(Vec::new());
    };
    let end = end.unwrap_or(start);
    let mut stmt = conn.prepare(
        "SELECT id, started_at, ended_at, source FROM matches WHERE playlist_id IS ?1 AND guid != ?2",
    )?;
    let rows = stmt.query_map(params![m.playlist_id, m.guid], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, s2, e2, src) = row?;
        if source.is_some_and(|want| src.as_deref() != Some(want)) {
            continue;
        }
        let Some(s2) = parse_time(&s2) else { continue };
        let e2 = e2.as_deref().and_then(parse_time).unwrap_or(s2);
        if start < e2 && s2 < end {
            out.push(id);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_match(guid: &str, start: &str, end: &str, a: i64, b: i64) -> MatchRecord {
        MatchRecord {
            guid: guid.into(),
            started_at: start.into(),
            ended_at: Some(end.into()),
            playlist_id: Some(11),
            my_team: Some(0),
            team0_score: Some(a),
            team1_score: Some(b),
            result: if a > b { "W" } else { "L" }.into(),
            ..Default::default()
        }
    }

    #[test]
    fn mmr_after_comes_from_next_queue_and_typed_values_win() {
        let db = Database::in_memory().unwrap();
        db.add_mmr_sample(11, 1356.0, Some(18), "2026-10-21T20:00:00+02:00")
            .unwrap();
        db.add_log_match(&log_match(
            "m1",
            "2026-10-21T20:01:00+02:00",
            "2026-10-21T20:06:00+02:00",
            3,
            1,
        ))
        .unwrap();
        db.add_mmr_sample(11, 1366.0, Some(18), "2026-10-21T20:07:00+02:00")
            .unwrap();
        db.add_log_match(&log_match(
            "m2",
            "2026-10-21T20:08:00+02:00",
            "2026-10-21T20:13:00+02:00",
            0,
            2,
        ))
        .unwrap();
        db.reconcile_mmr().unwrap();
        let ms = db.list_matches().unwrap();
        assert_eq!(ms[0]["mmr_before"], json!(1356.0));
        assert_eq!(ms[0]["mmr"], json!(1366.0));
        assert_eq!(ms[1]["mmr"], Value::Null);

        let id2 = ms[1]["id"].as_i64().unwrap();
        db.update_match(id2, Some(Some(1359.0)), None, None)
            .unwrap();
        db.add_mmr_sample(11, 1350.0, Some(18), "2026-10-21T20:30:00+02:00")
            .unwrap();
        db.reconcile_mmr().unwrap();
        assert_eq!(db.list_matches().unwrap()[1]["mmr"], json!(1359.0));
    }

    #[test]
    fn live_match_replaces_log_copy() {
        let db = Database::in_memory().unwrap();
        let log = log_match(
            "log-1",
            "2026-10-22T20:00:00+02:00",
            "2026-10-22T20:06:00+02:00",
            3,
            1,
        );
        assert!(db.add_log_match(&log).unwrap());
        assert!(!db.add_log_match(&log).unwrap());
        let live = MatchRecord {
            guid: "GUID".into(),
            started_at: "2026-10-22T20:00:05+02:00".into(),
            ended_at: Some("2026-10-22T20:05:58+02:00".into()),
            playlist_id: Some(11),
            result: "W".into(),
            goals: Some(2),
            ..Default::default()
        };
        db.upsert_match(&live).unwrap();
        let ms = db.list_matches().unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0]["source"], json!("api"));
        assert!(
            !db.add_log_match(&MatchRecord {
                guid: "log-2".into(),
                ..log
            })
            .unwrap()
        );
    }

    #[test]
    fn log_result_follows_mmr_change() {
        let db = Database::in_memory().unwrap();
        db.add_mmr_sample(11, 1400.0, None, "2026-10-21T20:00:00+02:00")
            .unwrap();
        // Score says win (opponents forfeited while ahead is impossible, but trust MMR anyway).
        db.add_log_match(&log_match(
            "m",
            "2026-10-21T20:01:00+02:00",
            "2026-10-21T20:06:00+02:00",
            2,
            1,
        ))
        .unwrap();
        db.add_mmr_sample(11, 1391.0, None, "2026-10-21T20:07:00+02:00")
            .unwrap();
        db.reconcile_mmr().unwrap();
        assert_eq!(db.list_matches().unwrap()[0]["result"], json!("L"));
    }
}
