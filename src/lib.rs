//! Local Rocket Stats: a match-by-match Rocket League stats and MMR tracker that reads only
//! local data from the game (the official Stats API socket and the game log).

pub mod logging;

pub mod collector;
pub mod db;
pub mod gamedata;
pub mod i18n;
pub mod logwatch;
pub mod paths;
pub mod platform;
pub mod runtime;
pub mod server;
pub mod settings;
pub mod setup;
pub mod simulator;
#[cfg(any(target_os = "linux", windows))]
pub mod tray;
