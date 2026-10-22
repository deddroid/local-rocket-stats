//! Tray and message texts in English and Italian (the web UI has its own, in `web/i18n.js`).

use crate::settings::Lang;

#[derive(Clone, Copy)]
pub enum T {
    WaitingGame,
    GameConnected,
    InMatch,
    StatsApiOff,
    OpenDashboard,
    PhoneAccess,
    Phone,
    EnableStatsApi,
    StartAtLogin,
    Language,
    LangAuto,
    OpenLog,
    Quit,
    StatsApiEnabled,
    RestartGame,
    AlreadyRunning,
}

pub fn t(lang: Lang, key: T) -> &'static str {
    use T::*;
    match (lang, key) {
        (Lang::En, WaitingGame) => "Waiting for the game",
        (Lang::It, WaitingGame) => "In attesa del gioco",
        (Lang::En, GameConnected) => "Game connected",
        (Lang::It, GameConnected) => "Gioco connesso",
        (Lang::En, InMatch) => "In match",
        (Lang::It, InMatch) => "In partita",
        (Lang::En, StatsApiOff) => "Stats API is off in the game",
        (Lang::It, StatsApiOff) => "Stats API disattivata nel gioco",
        (Lang::En, OpenDashboard) => "Open dashboard",
        (Lang::It, OpenDashboard) => "Apri dashboard",
        (Lang::En, PhoneAccess) => "Phone access (local network)",
        (Lang::It, PhoneAccess) => "Accesso dal telefono (rete locale)",
        (Lang::En, Phone) => "Phone",
        (Lang::It, Phone) => "Telefono",
        (Lang::En, EnableStatsApi) => "Enable the game's Stats API",
        (Lang::It, EnableStatsApi) => "Abilita la Stats API del gioco",
        (Lang::En, StartAtLogin) => "Start at login",
        (Lang::It, StartAtLogin) => "Avvia all'accesso",
        (Lang::En, Language) => "Language",
        (Lang::It, Language) => "Lingua",
        (Lang::En, LangAuto) => "System",
        (Lang::It, LangAuto) => "Sistema",
        (Lang::En, OpenLog) => "Open log",
        (Lang::It, OpenLog) => "Apri log",
        (Lang::En, Quit) => "Quit",
        (Lang::It, Quit) => "Esci",
        (Lang::En, StatsApiEnabled) => "Stats API enabled: restart Rocket League",
        (Lang::It, StatsApiEnabled) => "Stats API abilitata: riavvia Rocket League",
        (Lang::En, RestartGame) => "restart Rocket League",
        (Lang::It, RestartGame) => "riavvia Rocket League",
        (Lang::En, AlreadyRunning) => "already running: opening the dashboard",
        (Lang::It, AlreadyRunning) => "già in esecuzione: apro la dashboard",
    }
}
