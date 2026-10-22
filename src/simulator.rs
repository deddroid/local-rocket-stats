//! Fake Stats API server: plays random matches to try the app without the game.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

const ARENAS: [&str; 6] = [
    "Stadium_P",
    "EuroStadium_Night_P",
    "CS_P",
    "TrainStation_P",
    "UtopiaStadium_P",
    "Park_P",
];
const NAMES: [&str; 9] = [
    "Zephyr", "Kinetic", "Mango", "Rolly", "Nimbus", "Torque", "Pixel", "Saltine", "Vortex",
];

/// Small xorshift generator: good enough for fake matches, no extra dependency.
struct Rng(u64);

impl Rng {
    fn new() -> Self {
        Self(chrono::Utc::now().timestamp_nanos_opt().unwrap_or(1) as u64 | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn chance(&mut self, p: f64) -> bool {
        (self.next() % 1_000_000) as f64 / 1_000_000.0 < p
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[(self.next() % xs.len() as u64) as usize]
    }
}

fn send(conn: &mut TcpStream, event: &str, data: Value) -> std::io::Result<()> {
    conn.write_all(json!({"Event": event, "Data": data}).to_string().as_bytes())
}

fn r#ref(p: &Value) -> Value {
    json!({"Name": p["Name"], "Shortcut": p["Shortcut"], "TeamNum": p["TeamNum"]})
}

fn bump(p: &mut Value, key: &str, by: i64) {
    p[key] = json!(p[key].as_i64().unwrap_or(0) + by);
}

fn play(conn: &mut TcpStream, rng: &mut Rng, speed: f64, abandon: bool) -> std::io::Result<String> {
    let playlist = rng.pick(&[10, 11, 11, 13]);
    let size = match playlist {
        10 => 1,
        11 => 2,
        _ => 3,
    };
    let guid = format!("{:032X}", rng.next() as u128 * rng.next() as u128);
    let mut players: Vec<Value> =
        vec![json!({"Name": "You", "PrimaryId": "Epic|0123456789abcdef0123456789abcdef|0"})];
    let mut names = NAMES.to_vec();
    for _ in 0..size * 2 - 1 {
        let i = (rng.next() % names.len() as u64) as usize;
        players.push(json!({"Name": names.remove(i), "PrimaryId": format!("Steam|{}|0", 76561198000000000u64 + rng.next() % 1_000_000)}));
    }
    for (i, p) in players.iter_mut().enumerate() {
        for k in [
            "Score",
            "Goals",
            "Shots",
            "Assists",
            "Saves",
            "Touches",
            "CarTouches",
            "Demos",
        ] {
            p[k] = json!(0);
        }
        p["Shortcut"] = json!(i + 1);
        p["TeamNum"] = json!(if i < size { 0 } else { 1 });
    }
    let arena = rng.pick(&ARENAS);
    let mut teams = [0i64, 0];
    let (mut clock, mut overtime) = (300i64, false);
    let blue_odds = 0.4 + (rng.next() % 30) as f64 / 100.0;
    let stop_at = if abandon {
        60 + (rng.next() % 180) as i64
    } else {
        -1
    };

    send(conn, "MatchCreated", json!({"MatchGuid": guid}))?;
    for p in &players {
        send(
            conn,
            "PlayerJoined",
            json!({"MatchGuid": guid, "PlayerName": p["Name"], "PrimaryId": p["PrimaryId"]}),
        )?;
    }
    send(conn, "MatchInitialized", json!({"MatchGuid": guid}))?;
    send(conn, "RoundStarted", json!({"MatchGuid": guid}))?;
    while clock > 0 || overtime || teams[0] == teams[1] {
        if clock == 0 && teams[0] == teams[1] {
            overtime = true;
        }
        for p in players.iter_mut() {
            if rng.chance(0.08) {
                bump(p, "Touches", 1);
                bump(p, "Score", 2);
            }
        }
        if rng.chance(0.035) {
            let team = if rng.chance(blue_odds) { 0 } else { 1 };
            let mates: Vec<usize> = (0..players.len())
                .filter(|&i| players[i]["TeamNum"] == team)
                .collect();
            let scorer = rng.pick(&mates);
            bump(&mut players[scorer], "Goals", 1);
            bump(&mut players[scorer], "Shots", 1);
            bump(&mut players[scorer], "Score", 100);
            let mut goal = json!({"MatchGuid": guid, "GoalSpeed": 1500 + rng.next() % 2000, "Scorer": r#ref(&players[scorer])});
            if mates.len() > 1 && rng.chance(0.6) {
                let others: Vec<usize> = mates.iter().copied().filter(|&i| i != scorer).collect();
                let a = rng.pick(&others);
                bump(&mut players[a], "Assists", 1);
                bump(&mut players[a], "Score", 50);
                goal["Assister"] = r#ref(&players[a]);
            }
            teams[team as usize] += 1;
            send(conn, "GoalScored", goal)?;
            send(
                conn,
                "StatfeedEvent",
                json!({"MatchGuid": guid, "EventName": "Goal", "MainTarget": r#ref(&players[scorer])}),
            )?;
            if overtime {
                break;
            }
        }
        if rng.chance(0.03) {
            let i = (rng.next() % players.len() as u64) as usize;
            bump(&mut players[i], "Saves", 1);
            bump(&mut players[i], "Score", 50);
            let kind = if rng.chance(0.2) { "EpicSave" } else { "Save" };
            send(
                conn,
                "StatfeedEvent",
                json!({"MatchGuid": guid, "EventName": kind, "MainTarget": r#ref(&players[i])}),
            )?;
        }
        if rng.chance(0.02) {
            let i = (rng.next() % players.len() as u64) as usize;
            bump(&mut players[i], "Shots", 1);
        }
        let state = json!({"MatchGuid": guid, "Players": players, "Game": {
            "Teams": [{"Name": "Blue", "TeamNum": 0, "Score": teams[0]}, {"Name": "Orange", "TeamNum": 1, "Score": teams[1]}],
            "PlaylistId": playlist, "TimeSeconds": clock, "bOvertime": overtime, "bReplay": false,
            "Arena": arena, "bHasTarget": true, "Target": r#ref(&players[0])}});
        send(conn, "UpdateState", state)?;
        if clock == stop_at {
            send(conn, "MatchDestroyed", json!({"MatchGuid": guid}))?;
            return Ok("abandoned".into());
        }
        if !overtime {
            clock -= 1;
        }
        thread::sleep(Duration::from_secs_f64(1.0 / speed));
    }
    let winner = if teams[0] > teams[1] { 0 } else { 1 };
    send(
        conn,
        "MatchEnded",
        json!({"MatchGuid": guid, "WinnerTeamNum": winner}),
    )?;
    send(conn, "PodiumStart", json!({"MatchGuid": guid}))?;
    thread::sleep(Duration::from_secs_f64(2.0 / speed));
    send(conn, "MatchDestroyed", json!({"MatchGuid": guid}))?;
    Ok(format!("{}-{}", teams[0], teams[1]))
}

pub fn run(port: u16, matches: usize, speed: f64) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!("Fake Stats API on 127.0.0.1:{port}: waiting for Local Rocket Stats…");
    let (mut conn, _) = listener.accept()?;
    let mut rng = Rng::new();
    for i in 0..matches {
        let result = play(&mut conn, &mut rng, speed, matches > 3 && i == matches - 1)?;
        println!("Match {}/{matches}: {result}", i + 1);
        thread::sleep(Duration::from_secs_f64(3.0 / speed));
    }
    Ok(())
}
