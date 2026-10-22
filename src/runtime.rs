//! The running app: database, Stats API collector, log reader and web servers.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tiny_http::Server;

use crate::collector::Collector;
use crate::db::Database;
use crate::server::{self, App};
use crate::settings::{Lang, Settings};
use crate::{logln, paths};

pub struct Options {
    pub db: PathBuf,
    pub port: u16,
    pub game_port: u16,
    pub lan: bool,
    pub read_logs: bool,
}

pub struct Runtime {
    pub app: Arc<App>,
    pub port: u16,
    lan_server: Mutex<Option<Arc<Server>>>,
    /// Set after "Enable Stats API": the game must be restarted.
    pub stats_api_just_enabled: AtomicBool,
}

/// What the tray shows.
pub struct Status {
    pub connected: bool,
    pub in_match: bool,
    /// Your team first.
    pub score: Option<(i64, i64)>,
}

impl Runtime {
    pub fn start(opts: Options, settings: Settings) -> Result<Arc<Self>, String> {
        let db = Arc::new(
            Database::open(&opts.db).map_err(|e| format!("database {}: {e}", opts.db.display()))?,
        );
        let collector = Collector::new(Arc::clone(&db));
        let app = Arc::new(App {
            db: Arc::clone(&db),
            collector: Arc::clone(&collector),
            settings: Arc::new(Mutex::new(settings)),
        });
        // The local server first: if the port is taken, nothing else starts.
        server::spawn(Arc::clone(&app), ("127.0.0.1", opts.port))
            .map_err(|e| format!("port {}: {e}", opts.port))?;
        collector.spawn(opts.game_port);
        if opts.read_logs {
            crate::logwatch::spawn(db, Arc::clone(&collector), paths::log_dirs());
        }
        logln!(
            "Local Rocket Stats {} listening on {}",
            env!("CARGO_PKG_VERSION"),
            dashboard_url(opts.port, "/")
        );
        logln!("Database: {}", opts.db.display());
        let rt = Arc::new(Self {
            app,
            port: opts.port,
            lan_server: Mutex::new(None),
            stats_api_just_enabled: AtomicBool::new(false),
        });
        if opts.lan {
            if let Err(e) = rt.set_lan(true) {
                logln!("Phone access not available: {e}");
            }
        }
        Ok(rt)
    }

    pub fn settings(&self) -> Settings {
        self.app
            .settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) {
        let mut s = self.app.settings.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut s);
        if let Err(e) = s.save() {
            logln!("Could not save the settings: {e}");
        }
        self.app.collector.bump();
    }

    pub fn lang(&self) -> Lang {
        self.settings().lang()
    }

    pub fn lan_active(&self) -> bool {
        self.lan_server
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// Phone access: a second server on the LAN address (the first one stays on 127.0.0.1).
    /// Requests from other devices are read-only.
    pub fn set_lan(&self, on: bool) -> Result<(), String> {
        let mut slot = self.lan_server.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(srv) = slot.take() {
            srv.unblock();
        }
        if on {
            let ip = server::lan_ip().ok_or("no local network address")?;
            let srv = server::spawn(Arc::clone(&self.app), (ip.as_str(), self.port))
                .map_err(|e| e.to_string())?;
            *slot = Some(srv);
            logln!(
                "From the phone (same Wi-Fi): {}",
                self.phone_url().unwrap_or_default()
            );
        }
        Ok(())
    }

    pub fn phone_url(&self) -> Option<String> {
        server::lan_ip().map(|ip| format!("http://{ip}:{}/phone", self.port))
    }

    pub fn status(&self) -> Status {
        let live: Value = self.app.collector.live();
        let m = &live["match"];
        let score = m["score"].as_array().and_then(|a| {
            let (a, b) = (a.first()?.as_i64()?, a.get(1)?.as_i64()?);
            Some(if m["my_team"].as_i64() == Some(1) {
                (b, a)
            } else {
                (a, b)
            })
        });
        Status {
            connected: live["connected"].as_bool().unwrap_or(false),
            in_match: !m.is_null() && !m["players"].as_array().is_none_or(Vec::is_empty),
            score,
        }
    }

    pub fn enable_stats_api(&self) -> Result<(), String> {
        let files = crate::setup::run(None, 10)?;
        for f in files {
            logln!("Stats API enabled in {}", f.display());
        }
        self.stats_api_just_enabled.store(true, Ordering::SeqCst);
        Ok(())
    }
}

pub fn dashboard_url(port: u16, path: &str) -> String {
    format!("http://127.0.0.1:{port}{path}")
}

pub fn open(target: &str) {
    if let Err(e) = opener::open(target) {
        logln!("Could not open {target}: {e}");
    }
}
