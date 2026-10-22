//! Tray icon: what it shows and what its menu does, shared by Linux (`ksni`) and Windows
//! (`tray-icon`).

use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::i18n::{T, t};
use crate::logln;
use crate::platform::autostart;
use crate::runtime::{self, Runtime};
use crate::settings::Lang;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
pub use linux::run;
#[cfg(windows)]
pub use windows::run;

#[derive(Clone, Copy, Debug)]
pub enum Action {
    OpenDashboard,
    ToggleLan,
    EnableStatsApi,
    ToggleAutostart,
    SetLang(Option<Lang>),
    OpenLog,
    Quit,
}

/// Snapshot of everything the tray displays.
#[derive(Clone, PartialEq)]
pub struct View {
    pub lang: Lang,
    pub lang_auto: bool,
    pub state: String,
    pub connected: bool,
    pub in_match: bool,
    pub lan: bool,
    pub phone_url: Option<String>,
    pub stats_api_on: bool,
    pub autostart: bool,
}

impl View {
    pub fn build(rt: &Runtime) -> Self {
        let settings = rt.settings();
        let lang = settings.lang();
        let st = rt.status();
        if st.connected {
            rt.stats_api_just_enabled.store(false, Ordering::SeqCst);
        }
        let stats_api_on = st.connected || crate::setup::is_enabled();
        let state = if st.in_match {
            match st.score {
                Some((a, b)) => format!("{}: {a} – {b}", t(lang, T::InMatch)),
                None => t(lang, T::InMatch).into(),
            }
        } else if st.connected {
            t(lang, T::GameConnected).into()
        } else if rt.stats_api_just_enabled.load(Ordering::SeqCst) {
            t(lang, T::StatsApiEnabled).into()
        } else if !stats_api_on {
            t(lang, T::StatsApiOff).into()
        } else {
            t(lang, T::WaitingGame).into()
        };
        let lan = rt.lan_active();
        Self {
            lang,
            lang_auto: settings.lang.is_none(),
            state,
            connected: st.connected,
            in_match: st.in_match,
            lan,
            phone_url: if lan { rt.phone_url() } else { None },
            stats_api_on,
            autostart: autostart::is_enabled(),
        }
    }

    pub fn tooltip(&self) -> String {
        format!("Local Rocket Stats — {}", self.state)
    }
}

pub fn perform(rt: &Arc<Runtime>, action: Action) {
    match action {
        Action::OpenDashboard => runtime::open(&runtime::dashboard_url(rt.port, "/")),
        Action::ToggleLan => {
            let on = !rt.lan_active();
            match rt.set_lan(on) {
                Ok(()) => rt.update_settings(|s| s.lan = on),
                Err(e) => logln!("Phone access: {e}"),
            }
        }
        Action::EnableStatsApi => {
            if let Err(e) = rt.enable_stats_api() {
                logln!("Stats API: {e}");
            }
        }
        Action::ToggleAutostart => {
            if let Err(e) = autostart::set_enabled(!autostart::is_enabled()) {
                logln!("Start at login: {e}");
            }
        }
        Action::SetLang(lang) => rt.update_settings(|s| s.lang = lang),
        Action::OpenLog => runtime::open(&crate::settings::log_path().to_string_lossy()),
        Action::Quit => {
            logln!("Quit from the tray");
            std::process::exit(0);
        }
    }
}

/// The icon, drawn at runtime: a ball with a status dot (RGBA, row by row).
pub fn icon_rgba(size: u32, view: &View) -> Vec<u8> {
    let base = (61u8, 139u8, 253u8);
    let dot = if view.in_match {
        Some((255, 138, 61)) // orange: in a match
    } else if view.connected {
        Some((47, 191, 113)) // green: game connected
    } else if !view.stats_api_on {
        Some((229, 72, 77)) // red: the game won't send data
    } else {
        None
    };
    let s = size as f32;
    let (c, r) = (s / 2.0, s / 2.0 - 1.0);
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let d = (fx * fx + fy * fy).sqrt();
            let mut a = ((r - d + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
            // A hexagon and a ring, like the ball's panels.
            let hex = fx.abs() * 0.866 + fy.abs() * 0.5 < r * 0.36 && fy.abs() < r * 0.36;
            let ring = (d - r * 0.62).abs() < s * 0.035;
            let mut px = if hex || ring { (245, 247, 250) } else { base };
            if let Some(dc) = dot {
                let (dx, dy) = (x as f32 + 0.5 - s * 0.78, y as f32 + 0.5 - s * 0.78);
                let dd = (dx * dx + dy * dy).sqrt();
                let dr = s * 0.2;
                if dd < dr + 1.5 {
                    px = if dd < dr { dc } else { (20, 24, 32) };
                    a = 255;
                }
            }
            data.extend_from_slice(&[px.0, px.1, px.2, a]);
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_transparent_corners_and_opaque_center() {
        let view = View {
            lang: Lang::En,
            lang_auto: true,
            state: String::new(),
            connected: true,
            in_match: false,
            lan: false,
            phone_url: None,
            stats_api_on: true,
            autostart: false,
        };
        let px = icon_rgba(32, &view);
        assert_eq!(px.len(), 32 * 32 * 4);
        assert_eq!(px[3], 0);
        assert_eq!(px[(16 * 32 + 16) * 4 + 3], 255);
    }
}
