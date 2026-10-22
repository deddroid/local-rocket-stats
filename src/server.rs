//! Local web server: dashboard, phone page, OBS overlay and JSON API.

use std::io::Read;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server};

use crate::collector::Collector;
use crate::db::Database;
use crate::gamedata::{self, ARENAS, PLAYLISTS, RANKED};
use crate::settings::{Lang, Settings};

/// Web files, embedded in the executable.
const FILES: &[(&str, &str, &str)] = &[
    (
        "/index.html",
        "text/html; charset=utf-8",
        include_str!("../web/index.html"),
    ),
    (
        "/phone.html",
        "text/html; charset=utf-8",
        include_str!("../web/phone.html"),
    ),
    (
        "/overlay.html",
        "text/html; charset=utf-8",
        include_str!("../web/overlay.html"),
    ),
    (
        "/app.js",
        "text/javascript; charset=utf-8",
        include_str!("../web/app.js"),
    ),
    (
        "/common.js",
        "text/javascript; charset=utf-8",
        include_str!("../web/common.js"),
    ),
    (
        "/i18n.js",
        "text/javascript; charset=utf-8",
        include_str!("../web/i18n.js"),
    ),
    (
        "/ranks.js",
        "text/javascript; charset=utf-8",
        include_str!("../web/ranks.js"),
    ),
    (
        "/style.css",
        "text/css; charset=utf-8",
        include_str!("../web/style.css"),
    ),
    (
        "/icon.svg",
        "image/svg+xml",
        include_str!("../assets/icon.svg"),
    ),
];

pub struct App {
    pub db: Arc<Database>,
    pub collector: Arc<Collector>,
    pub settings: Arc<Mutex<Settings>>,
}

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k, v).expect("valid header")
}

fn json_response(v: &Value, status: u16) -> Response<std::io::Cursor<Vec<u8>>> {
    Response::from_data(v.to_string().into_bytes())
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"))
}

fn is_local(req: &Request) -> bool {
    req.remote_addr().is_some_and(|a| match a.ip() {
        IpAddr::V4(ip) => ip.is_loopback(),
        IpAddr::V6(ip) => {
            ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
    })
}

/// Writes are allowed only from this computer, and not from other websites open in the browser.
fn write_allowed(req: &Request) -> Result<(), &'static str> {
    if !is_local(req) {
        return Err("read-only from other devices");
    }
    let get = |name: &str| {
        req.headers()
            .iter()
            .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str().to_string())
    };
    if let (Some(origin), Some(host)) = (get("Origin"), get("Host")) {
        if !origin.ends_with(&format!("//{host}")) {
            return Err("origin not allowed");
        }
    }
    Ok(())
}

impl App {
    fn meta(&self) -> Value {
        let s = self
            .settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "playlists": PLAYLISTS.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
            "ranked": RANKED,
            "arenas": ARENAS.iter().map(|(k, v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
            "me_ids": self.db.me_ids(),
            "session_gap_min": self.db.get_setting("session_gap_min").and_then(|v| v.as_i64()).unwrap_or(30),
            "lang": s.lang().code(),
            "lang_auto": s.lang.is_none(),
            "lan": s.lan,
        })
    }

    fn export_csv(&self) -> String {
        let mut out = String::from(
            "id,started_at,ended_at,playlist,arena,result,my_score,opp_score,overtime,score,goals,assists,saves,shots,demos,touches,mmr,source,notes\n",
        );
        let cell = |v: &Value| -> String {
            let s = match v {
                Value::Null => String::new(),
                Value::String(s) => s.clone(),
                v => v.to_string(),
            };
            if s.contains([',', '"', '\n']) {
                format!("\"{}\"", s.replace('"', "\"\""))
            } else {
                s
            }
        };
        for m in self.db.list_matches().unwrap_or_default() {
            let (mut mine, mut opp) = (m["team0_score"].clone(), m["team1_score"].clone());
            if m["my_team"].as_i64() == Some(1) {
                std::mem::swap(&mut mine, &mut opp);
            }
            let row = [
                m["id"].clone(),
                m["started_at"].clone(),
                m["ended_at"].clone(),
                json!(gamedata::playlist_name(m["playlist_id"].as_i64())),
                json!(gamedata::arena_name(m["arena"].as_str())),
                m["result"].clone(),
                mine,
                opp,
                m["overtime"].clone(),
                m["score"].clone(),
                m["goals"].clone(),
                m["assists"].clone(),
                m["saves"].clone(),
                m["shots"].clone(),
                m["demos"].clone(),
                m["touches"].clone(),
                m["mmr"].clone(),
                m["source"].clone(),
                m["notes"].clone(),
            ];
            out.push_str(&row.iter().map(cell).collect::<Vec<_>>().join(","));
            out.push('\n');
        }
        out
    }

    /// Screenshot mode: `?preload=1` embeds the API data in the page so it is complete as soon as
    /// it loads (headless browsers capture at the load event); `?theme=dark|light` forces a theme.
    fn page(&self, body: &str, query: &str) -> String {
        let param = |k: &str| {
            query
                .split('&')
                .find_map(|kv| kv.strip_prefix(k)?.strip_prefix('='))
        };
        let mut html = body.to_string();
        if let Some(theme) = param("theme").filter(|t| ["dark", "light"].contains(t)) {
            html = html.replacen("<html ", &format!("<html data-theme=\"{theme}\" "), 1);
        }
        if param("preload") == Some("1") {
            let data = json!({
                "/api/meta": self.meta(),
                "/api/matches": self.db.list_matches().unwrap_or_default(),
                "/api/ranks": self.db.latest_ranks().unwrap_or(json!({})),
                "/api/live": self.collector.live(),
            });
            // `</` can't appear inside the inline script.
            let script = format!(
                "<script>window.__PRELOAD__ = {};</script>\n",
                data.to_string().replace("</", "<\\/")
            );
            html = html.replacen(
                "<script type=\"module\"",
                &format!("{script}<script type=\"module\""),
                1,
            );
        }
        html
    }

    fn handle(&self, mut req: Request) {
        let full = req.url().to_string();
        let (path, query) = full.split_once('?').unwrap_or((&full, ""));
        let url = path.to_string();
        let method = req.method().clone();
        let resp = match (&method, url.as_str()) {
            (Method::Get, "/api/live") => json_response(&self.collector.live(), 200),
            (Method::Get, "/api/matches") => {
                json_response(&json!(self.db.list_matches().unwrap_or_default()), 200)
            }
            (Method::Get, "/api/ranks") => {
                json_response(&self.db.latest_ranks().unwrap_or(json!({})), 200)
            }
            (Method::Get, "/api/meta") => json_response(&self.meta(), 200),
            (Method::Get, "/api/export.csv") => Response::from_data(self.export_csv().into_bytes())
                .with_header(header("Content-Type", "text/csv; charset=utf-8"))
                .with_header(header(
                    "Content-Disposition",
                    "attachment; filename=\"local-rocket-stats.csv\"",
                )),
            (Method::Get, path) => {
                let path = match path {
                    "/" => "/index.html",
                    "/phone" | "/m" => "/phone.html",
                    "/overlay" => "/overlay.html",
                    p => p,
                };
                match FILES.iter().find(|(p, _, _)| *p == path) {
                    Some((p, ctype, body)) if p.ends_with(".html") => {
                        Response::from_data(self.page(body, query).into_bytes())
                            .with_header(header("Content-Type", ctype))
                            .with_header(header("Cache-Control", "no-cache"))
                    }
                    Some((_, ctype, body)) => Response::from_data(body.as_bytes().to_vec())
                        .with_header(header("Content-Type", ctype))
                        .with_header(header("Cache-Control", "no-cache")),
                    None => json_response(&json!({"error": "not found"}), 404),
                }
            }
            (Method::Post | Method::Delete, _) => match write_allowed(&req) {
                Err(e) => json_response(&json!({"error": e}), 403),
                Ok(()) => {
                    let mut body = String::new();
                    let _ = req.as_reader().take(1 << 20).read_to_string(&mut body);
                    let body: Value = if body.trim().is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(&body).unwrap_or(Value::Null)
                    };
                    self.write(&method, &url, &body)
                }
            },
            _ => json_response(&json!({"error": "not found"}), 404),
        };
        let _ = req.respond(resp);
    }

    fn write(
        &self,
        method: &Method,
        url: &str,
        body: &Value,
    ) -> Response<std::io::Cursor<Vec<u8>>> {
        if body.is_null() {
            return json_response(&json!({"error": "invalid JSON"}), 400);
        }
        let match_id = url
            .strip_prefix("/api/matches/")
            .and_then(|s| s.parse::<i64>().ok());
        match (method, match_id) {
            (Method::Delete, Some(id)) => {
                let ok = self.db.delete_match(id).unwrap_or(false);
                self.collector.bump();
                json_response(&json!({"ok": ok}), if ok { 200 } else { 404 })
            }
            (Method::Post, Some(id)) => {
                let mmr = match body.get("mmr") {
                    None => None,
                    Some(Value::Null) => Some(None),
                    Some(Value::String(s)) if s.is_empty() => Some(None),
                    Some(v) => match v
                        .as_f64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                    {
                        Some(f) => Some(Some(f)),
                        None => return json_response(&json!({"error": "invalid MMR"}), 400),
                    },
                };
                let notes = body.get("notes").map(|v| {
                    v.as_str()
                        .filter(|s| !s.is_empty())
                        .map(|s| s.chars().take(2000).collect())
                });
                let result = body
                    .get("result")
                    .and_then(Value::as_str)
                    .filter(|r| ["W", "L", "A", "?"].contains(r))
                    .map(str::to_string);
                let ok = self
                    .db
                    .update_match(id, mmr, notes, result)
                    .unwrap_or(false);
                self.collector.bump();
                json_response(&json!({"ok": ok}), if ok { 200 } else { 404 })
            }
            (Method::Post, None) if url == "/api/settings" => {
                if let Some(ids) = body.get("me_ids").and_then(Value::as_array) {
                    let ids: Vec<String> = ids
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect();
                    let _ = self.db.set_setting("me_ids", &json!(ids));
                }
                if let Some(gap) = body.get("session_gap_min") {
                    match gap.as_i64() {
                        Some(g) => {
                            let _ = self.db.set_setting("session_gap_min", &json!(g.max(5)));
                        }
                        None => return json_response(&json!({"error": "invalid value"}), 400),
                    }
                }
                if let Some(lang) = body.get("lang") {
                    let mut s = self.settings.lock().unwrap_or_else(|e| e.into_inner());
                    s.lang = lang.as_str().and_then(Lang::from_code);
                    let _ = s.save();
                }
                self.collector.bump();
                json_response(&json!({"ok": true}), 200)
            }
            _ => json_response(&json!({"error": "not found"}), 404),
        }
    }
}

/// Starts a server on `addr`; fails if the port is taken (e.g. the app is already running).
/// `unblock()` on the returned handle stops it.
pub fn spawn(app: Arc<App>, addr: (&str, u16)) -> std::io::Result<Arc<Server>> {
    let server = Arc::new(
        Server::http(addr)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::AddrInUse, e.to_string()))?,
    );
    let srv = Arc::clone(&server);
    thread::Builder::new().name("http".into()).spawn(move || {
        while let Ok(req) = srv.recv() {
            let app = Arc::clone(&app);
            thread::spawn(move || app.handle(req));
        }
    })?;
    Ok(server)
}

/// IPv4 address of this machine on the local network.
pub fn lan_ip() -> Option<String> {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("192.0.2.1:9").ok()?; // nothing is sent: it only picks the interface
    let ip = sock.local_addr().ok()?.ip();
    (!ip.is_loopback()).then(|| ip.to_string())
}
