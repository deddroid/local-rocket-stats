//! Client for the Rocket League Stats API (local TCP socket) and the match state machine.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use crate::db::{Database, MatchRecord};
use crate::gamedata;
use crate::logln;

const STAT_KEYS: [&str; 7] = [
    "Score", "Goals", "Assists", "Saves", "Shots", "Demos", "Touches",
];

pub fn now_iso() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// Splits a byte stream into consecutive JSON objects, with or without separators, even when
/// a TCP packet ends in the middle of an object or of a string escape.
#[derive(Default)]
pub struct JsonStream {
    buf: Vec<u8>,
}

impl JsonStream {
    const MAX_BUFFER: usize = 8 << 20;

    pub fn feed(&mut self, data: &[u8]) -> Vec<Value> {
        self.buf.extend_from_slice(data);
        let mut out = Vec::new();
        loop {
            let Some(start) = self.buf.iter().position(|&b| b == b'{') else {
                self.buf.clear();
                break;
            };
            let Some(end) = object_end(&self.buf, start) else {
                // Incomplete object: wait for more data.
                self.buf.drain(..start);
                if self.buf.len() > Self::MAX_BUFFER {
                    self.buf.clear();
                }
                break;
            };
            if let Ok(v) = serde_json::from_slice::<Value>(&self.buf[start..end]) {
                out.push(v);
            }
            self.buf.drain(..end);
        }
        out
    }
}

/// Index just after the brace that closes the object opened at `start`.
fn object_end(buf: &[u8], start: usize) -> Option<usize> {
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for (i, &c) in buf.iter().enumerate().skip(start) {
        if in_str {
            if esc {
                esc = false;
            } else if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }
    }
    None
}

/// Stable key for a player: PrimaryId, or the name for bots and unknown ids.
fn player_key(p: &Value) -> String {
    let pid = p["PrimaryId"].as_str().unwrap_or("");
    let uid = pid.split('|').nth(1);
    if pid.is_empty() || pid.to_lowercase().starts_with("unknown") || uid == Some("0") {
        format!("name:{}", p["Name"].as_str().unwrap_or("?"))
    } else {
        pid.to_string()
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Player {
    pub name: String,
    pub id: Option<String>,
    pub team: i64,
    pub score: i64,
    pub goals: i64,
    pub assists: i64,
    pub saves: i64,
    pub shots: i64,
    pub demos: i64,
    pub touches: i64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub left: bool,
}

struct Match {
    guid: String,
    online: bool,
    started_at: String,
    ended_at: Option<String>,
    playlist_id: Option<i64>,
    arena: Option<String>,
    teams: HashMap<i64, i64>,
    time_left: Option<i64>,
    overtime: bool,
    players: BTreeMap<String, Player>,
    left: HashSet<String>,
    goals: Vec<Value>,
    statfeed: HashMap<String, BTreeMap<String, i64>>,
    targets: HashMap<String, u32>,
    round_started: bool,
    winner: Option<i64>,
    ended: bool,
    saved_sig: Option<String>,
}

impl Match {
    fn new(guid: &str) -> Self {
        let online = !guid.is_empty();
        let guid = if online {
            guid.to_string()
        } else {
            format!(
                "local-{:x}{:x}",
                chrono::Utc::now().timestamp_micros(),
                std::process::id()
            )
        };
        Self {
            guid,
            online,
            started_at: now_iso(),
            ended_at: None,
            playlist_id: None,
            arena: None,
            teams: HashMap::new(),
            time_left: None,
            overtime: false,
            players: BTreeMap::new(),
            left: HashSet::new(),
            goals: Vec::new(),
            statfeed: HashMap::new(),
            targets: HashMap::new(),
            round_started: false,
            winner: None,
            ended: false,
            saved_sig: None,
        }
    }

    fn sorted_players(&self) -> Vec<Player> {
        let mut ps: Vec<Player> = self.players.values().cloned().collect();
        ps.sort_by_key(|p| (p.team, -p.score));
        ps
    }
}

/// Live state shared with the web server and the tray.
pub struct Collector {
    db: Arc<Database>,
    state: Mutex<State>,
    pub connected: AtomicBool,
    /// Bumped on every saved change, so the dashboard knows when to reload.
    pub version: AtomicU64,
}

struct State {
    current: Option<Match>,
    in_replay: bool,
}

impl Collector {
    pub fn new(db: Arc<Database>) -> Arc<Self> {
        Arc::new(Self {
            db,
            state: Mutex::new(State {
                current: None,
                in_replay: false,
            }),
            connected: AtomicBool::new(false),
            version: AtomicU64::new(0),
        })
    }

    pub fn bump(&self) {
        self.version.fetch_add(1, Ordering::SeqCst);
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Connects to the game and keeps reconnecting while the app runs.
    pub fn spawn(self: &Arc<Self>, port: u16) {
        let this = Arc::clone(self);
        thread::Builder::new()
            .name("collector".into())
            .spawn(move || {
                let addr: SocketAddr = ([127, 0, 0, 1], port).into();
                loop {
                    if let Ok(mut sock) = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
                    {
                        let _ = sock.set_read_timeout(Some(Duration::from_secs(1)));
                        this.connected.store(true, Ordering::SeqCst);
                        logln!("Connected to the Stats API on 127.0.0.1:{port}");
                        let mut stream = JsonStream::default();
                        let mut buf = vec![0u8; 65536];
                        loop {
                            match sock.read(&mut buf) {
                                Ok(0) => break,
                                Ok(n) => {
                                    for msg in stream.feed(&buf[..n]) {
                                        this.handle(&msg);
                                    }
                                }
                                Err(e)
                                    if matches!(
                                        e.kind(),
                                        std::io::ErrorKind::WouldBlock
                                            | std::io::ErrorKind::TimedOut
                                    ) => {}
                                Err(_) => break,
                            }
                        }
                        logln!("Disconnected from the Stats API");
                        this.connected.store(false, Ordering::SeqCst);
                        let mut st = this.state();
                        this.close_match(&mut st);
                    }
                    thread::sleep(Duration::from_secs(3));
                }
            })
            .expect("collector thread");
    }

    pub fn handle(&self, msg: &Value) {
        let Some(event) = msg["Event"].as_str() else {
            return;
        };
        // Some versions send Data as a JSON string.
        let data = match &msg["Data"] {
            Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
            v => v.clone(),
        };
        let mut st = self.state();
        self.dispatch(&mut st, event, &data);
    }

    fn dispatch(&self, st: &mut State, event: &str, data: &Value) {
        match event {
            "ReplayCreated" => {
                self.close_match(st);
                st.in_replay = true;
                return;
            }
            "MatchDestroyed" => {
                self.close_match(st);
                st.in_replay = false;
                return;
            }
            _ if st.in_replay => return,
            _ => {}
        }
        let guid = data["MatchGuid"].as_str().unwrap_or("");
        if let Some(m) = &st.current {
            if !guid.is_empty() && m.online && guid != m.guid {
                self.close_match(st);
            }
        }
        if let Some(m) = &mut st.current {
            if !guid.is_empty() && !m.online {
                m.guid = guid.to_string();
                m.online = true;
            }
        }
        if st.current.is_none() {
            if matches!(event, "MatchEnded" | "PodiumStart") {
                return;
            }
            st.current = Some(Match::new(guid));
        }
        let m = st.current.as_mut().expect("current match");
        match event {
            "UpdateState" => {
                on_update(m, data);
                if m.ended {
                    // Final updates after MatchEnded: save again if something changed.
                    self.save(m, false);
                }
            }
            "RoundStarted" => m.round_started = true,
            "GoalScored" => m.goals.push(json!({
                "scorer": data["Scorer"]["Name"],
                "assister": data["Assister"]["Name"],
                "team": data["Scorer"]["TeamNum"],
                "speed_kph": (data["GoalSpeed"].as_f64().unwrap_or(0.0) * 0.36).round() / 10.0,
                "time_left": m.time_left,
                "overtime": m.overtime,
            })),
            "StatfeedEvent" => {
                let who = data["MainTarget"]["Name"].as_str();
                let name = data["EventName"].as_str().or(data["Type"].as_str());
                if let (Some(who), Some(name)) = (who, name) {
                    *m.statfeed
                        .entry(who.into())
                        .or_default()
                        .entry(name.into())
                        .or_default() += 1;
                }
            }
            "PlayerLeft" => {
                if let Some(id) = data["PrimaryId"].as_str().or(data["PlayerName"].as_str()) {
                    m.left.insert(id.to_string());
                }
            }
            "MatchEnded" => {
                m.winner = data["WinnerTeamNum"].as_i64();
                m.ended = true;
                m.ended_at = Some(now_iso());
                self.save(m, false);
            }
            _ => {}
        }
    }

    fn close_match(&self, st: &mut State) {
        if let Some(mut m) = st.current.take() {
            if !m.ended && m.round_started {
                m.ended_at = Some(now_iso());
                self.save(&mut m, true);
            }
        }
    }

    /// Finds the local player: a saved PrimaryId, otherwise the car the camera follows most.
    fn find_me(&self, m: &Match) -> Option<Player> {
        let my_ids = self.db.me_ids();
        if let Some(p) = m
            .players
            .values()
            .find(|p| p.id.as_ref().is_some_and(|id| my_ids.contains(id)))
        {
            return Some(p.clone());
        }
        let (name, ticks) = m.targets.iter().max_by_key(|(_, t)| **t)?;
        let (key, p) = m.players.iter().find(|(_, p)| &p.name == name)?;
        // Remember the identity once it is reliable.
        if *ticks >= 10 && my_ids.is_empty() && !key.starts_with("name:") {
            if let Some(id) = &p.id {
                let _ = self.db.set_setting("me_ids", &json!([id]));
                logln!("Detected player: {} ({id})", p.name);
            }
        }
        Some(p.clone())
    }

    fn save(&self, m: &mut Match, abandoned: bool) {
        if gamedata::is_ignored(m.playlist_id) || m.players.len() < 2 {
            return;
        }
        let me = self.find_me(m);
        let my_team = me.as_ref().map(|p| p.team);
        let result = if abandoned {
            "A"
        } else {
            match (m.winner, my_team) {
                (Some(w), Some(t)) if w == t => "W",
                (Some(_), Some(_)) => "L",
                _ => "?",
            }
        };
        let mut players = m.sorted_players();
        for p in &mut players {
            p.left =
                p.id.as_ref().is_some_and(|id| m.left.contains(id)) || m.left.contains(&p.name);
        }
        let stat = |f: fn(&Player) -> i64| me.as_ref().map(f);
        let rec = MatchRecord {
            guid: m.guid.clone(),
            started_at: m.started_at.clone(),
            ended_at: m.ended_at.clone(),
            playlist_id: m.playlist_id,
            arena: m.arena.clone(),
            my_team,
            team0_score: m.teams.get(&0).copied(),
            team1_score: m.teams.get(&1).copied(),
            winner_team: m.winner,
            result: result.into(),
            overtime: m.overtime,
            me_id: me.as_ref().and_then(|p| p.id.clone()),
            me_name: me.as_ref().map(|p| p.name.clone()),
            score: stat(|p| p.score),
            goals: stat(|p| p.goals),
            assists: stat(|p| p.assists),
            saves: stat(|p| p.saves),
            shots: stat(|p| p.shots),
            demos: stat(|p| p.demos),
            touches: stat(|p| p.touches),
            players: serde_json::to_value(&players).ok(),
            goals_log: Some(Value::Array(m.goals.clone())),
            statfeed: me
                .as_ref()
                .map(|p| json!(m.statfeed.get(&p.name).cloned().unwrap_or_default())),
        };
        let sig = serde_json::to_string(&rec).unwrap_or_default();
        if m.saved_sig.as_deref() == Some(&sig) {
            return;
        }
        m.saved_sig = Some(sig);
        match self.db.upsert_match(&rec) {
            Ok(_) => {
                let _ = self.db.reconcile_mmr();
                self.bump();
                let n = |v: Option<i64>| v.map_or("?".into(), |v| v.to_string());
                logln!(
                    "Match saved: {result} {}-{} ({})",
                    n(rec.team0_score),
                    n(rec.team1_score),
                    rec.guid
                );
            }
            Err(e) => logln!("Could not save the match: {e}"),
        }
    }

    /// Live state for the dashboard (`/api/live`).
    pub fn live(&self) -> Value {
        let st = self.state();
        let mut out = json!({
            "connected": self.connected.load(Ordering::SeqCst),
            "version": self.version.load(Ordering::SeqCst),
            "in_replay": st.in_replay,
            "match": null,
        });
        if let Some(m) = &st.current {
            let me = if m.players.is_empty() {
                None
            } else {
                self.find_me(m)
            };
            out["match"] = json!({
                "playlist_id": m.playlist_id,
                "arena": m.arena,
                "time_left": m.time_left,
                "overtime": m.overtime,
                "score": [m.teams.get(&0).copied().unwrap_or(0), m.teams.get(&1).copied().unwrap_or(0)],
                "my_team": me.as_ref().map(|p| p.team),
                "me": me,
                "players": m.sorted_players(),
                "ended": m.ended,
            });
        }
        out
    }
}

fn on_update(m: &mut Match, data: &Value) {
    let game = &data["Game"];
    if let Some(p) = game["PlaylistId"].as_i64() {
        m.playlist_id = Some(p);
    }
    if let Some(a) = game["Arena"].as_str().filter(|a| !a.is_empty()) {
        m.arena = Some(a.to_string());
    }
    if let Some(t) = game["TimeSeconds"].as_i64() {
        m.time_left = Some(t);
    }
    if let Some(o) = game["bOvertime"].as_bool() {
        m.overtime = o;
    }
    for t in game["Teams"].as_array().into_iter().flatten() {
        if let Some(n) = t["TeamNum"].as_i64() {
            m.teams.insert(n, t["Score"].as_i64().unwrap_or(0));
        }
    }
    for p in data["Players"].as_array().into_iter().flatten() {
        let stat = |k: &str| p[k].as_i64().unwrap_or(0);
        let [score, goals, assists, saves, shots, demos, touches] = STAT_KEYS.map(stat);
        m.players.insert(
            player_key(p),
            Player {
                name: p["Name"].as_str().unwrap_or("?").to_string(),
                id: p["PrimaryId"].as_str().map(str::to_string),
                team: stat("TeamNum"),
                score,
                goals,
                assists,
                saves,
                shots,
                demos,
                touches,
                left: false,
            },
        );
    }
    let target = game["Target"]["Name"].as_str().unwrap_or("");
    if game["bHasTarget"].as_bool() == Some(true)
        && !target.is_empty()
        && game["bReplay"].as_bool() != Some(true)
    {
        *m.targets.entry(target.to_string()).or_default() += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_handles_split_packets_and_escapes() {
        let msgs = [
            json!({"Event": "A", "Data": {"x": "é}{\"\\"}}),
            json!({"Event": "B", "Data": json!({"y": 1}).to_string()}),
            json!({"Event": "C", "Data": {"n": [1, {"z": 2}]}}),
        ];
        let mut raw = msgs
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes();
        raw.push(0);
        raw.extend(msgs[0].to_string().bytes());
        for size in [1, 3, 7, 50, 9999] {
            let mut s = JsonStream::default();
            let out: Vec<Value> = raw.chunks(size).flat_map(|c| s.feed(c)).collect();
            let events: Vec<_> = out.iter().map(|o| o["Event"].as_str().unwrap()).collect();
            assert_eq!(events, ["A", "B", "C", "A"], "chunk size {size}");
            assert_eq!(out[0], msgs[0]);
        }
        let out = JsonStream::default().feed(br#"garbage}{"Event":"D"}"#);
        assert_eq!(out[0]["Event"], "D");
    }

    fn update(guid: &str, score: (i64, i64), me_goals: i64) -> Value {
        json!({"Event": "UpdateState", "Data": {"MatchGuid": guid,
            "Players": [
                {"Name": "Me", "PrimaryId": "Steam|76561198000000000|0", "TeamNum": 1, "Score": 100, "Goals": me_goals},
                {"Name": "Bot", "PrimaryId": "Unknown|0|0", "TeamNum": 0, "Score": 50}
            ],
            "Game": {"Teams": [{"TeamNum": 0, "Score": score.0}, {"TeamNum": 1, "Score": score.1}],
                     "PlaylistId": 11, "TimeSeconds": 100, "Arena": "Stadium_P",
                     "bHasTarget": true, "Target": {"Name": "Me"}}}})
    }

    #[test]
    fn match_lifecycle_saves_win_and_identity() {
        let db = Arc::new(Database::in_memory().unwrap());
        let c = Collector::new(Arc::clone(&db));
        c.handle(&json!({"Event": "MatchCreated", "Data": {"MatchGuid": "G1"}}));
        c.handle(&json!({"Event": "RoundStarted", "Data": {"MatchGuid": "G1"}}));
        for _ in 0..12 {
            c.handle(&update("G1", (1, 2), 2));
        }
        c.handle(&json!({"Event": "MatchEnded", "Data": {"MatchGuid": "G1", "WinnerTeamNum": 1}}));
        c.handle(&json!({"Event": "MatchDestroyed", "Data": {"MatchGuid": "G1"}}));
        let ms = db.list_matches().unwrap();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0]["result"], "W");
        assert_eq!(ms[0]["goals"], 2);
        assert_eq!(db.me_ids(), ["Steam|76561198000000000|0"]);
    }

    #[test]
    fn leaving_early_is_abandoned() {
        let db = Arc::new(Database::in_memory().unwrap());
        let c = Collector::new(Arc::clone(&db));
        c.handle(&json!({"Event": "RoundStarted", "Data": {"MatchGuid": "G2"}}));
        c.handle(&update("G2", (3, 0), 0));
        c.handle(&json!({"Event": "MatchDestroyed", "Data": {"MatchGuid": "G2"}}));
        assert_eq!(db.list_matches().unwrap()[0]["result"], "A");
    }
}
