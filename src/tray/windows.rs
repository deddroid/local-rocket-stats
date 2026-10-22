//! Windows notification-area icon via `tray-icon`, driven by a plain Win32 message loop.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{mem, ptr};

use tray_icon::menu::{
    CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, SetTimer, TranslateMessage,
};

use super::{Action, View, perform};
use crate::i18n::{T, t};
use crate::runtime::Runtime;
use crate::settings::Lang;

fn make_icon(view: &View) -> Option<Icon> {
    Icon::from_rgba(super::icon_rgba(32, view), 32, 32).ok()
}

/// Builds the menu for the current view, with the action of each clickable item.
fn build_menu(v: &View) -> (Menu, HashMap<MenuId, Action>) {
    let l = v.lang;
    let menu = Menu::new();
    let mut actions = HashMap::new();
    let mut add = |item: &dyn tray_icon::menu::IsMenuItem, id: Option<(MenuId, Action)>| {
        let _ = menu.append(item);
        if let Some((id, a)) = id {
            actions.insert(id, a);
        }
    };
    add(&MenuItem::new(&v.state, false, None), None);
    add(&PredefinedMenuItem::separator(), None);
    let open = MenuItem::new(t(l, T::OpenDashboard), true, None);
    add(&open, Some((open.id().clone(), Action::OpenDashboard)));
    if !v.stats_api_on {
        let en = MenuItem::new(t(l, T::EnableStatsApi), true, None);
        add(&en, Some((en.id().clone(), Action::EnableStatsApi)));
    }
    let lan = CheckMenuItem::new(t(l, T::PhoneAccess), true, v.lan, None);
    add(&lan, Some((lan.id().clone(), Action::ToggleLan)));
    if let Some(url) = &v.phone_url {
        add(
            &MenuItem::new(format!("{}: {url}", t(l, T::Phone)), false, None),
            None,
        );
    }
    let auto = CheckMenuItem::new(t(l, T::StartAtLogin), true, v.autostart, None);
    add(&auto, Some((auto.id().clone(), Action::ToggleAutostart)));

    let langs = Submenu::new(t(l, T::Language), true);
    for (label, choice) in [
        (t(l, T::LangAuto), None),
        ("English", Some(Lang::En)),
        ("Italiano", Some(Lang::It)),
    ] {
        let checked = if v.lang_auto {
            choice.is_none()
        } else {
            choice == Some(l)
        };
        let it = CheckMenuItem::new(label, true, checked, None);
        let _ = langs.append(&it);
        actions.insert(it.id().clone(), Action::SetLang(choice));
    }
    let _ = menu.append(&langs);

    let log = MenuItem::new(t(l, T::OpenLog), true, None);
    let _ = menu.append(&log);
    actions.insert(log.id().clone(), Action::OpenLog);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let quit = MenuItem::new(t(l, T::Quit), true, None);
    let _ = menu.append(&quit);
    actions.insert(quit.id().clone(), Action::Quit);
    (menu, actions)
}

fn apply(tray: &TrayIcon, view: &View) -> HashMap<MenuId, Action> {
    let (menu, actions) = build_menu(view);
    tray.set_menu(Some(Box::new(menu)));
    let _ = tray.set_icon(make_icon(view));
    let _ = tray.set_tooltip(Some(view.tooltip()));
    actions
}

pub fn run(rt: Arc<Runtime>) -> Result<(), String> {
    let mut view = View::build(&rt);
    let (menu, mut actions) = build_menu(&view);
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip(view.tooltip())
        .with_icon(make_icon(&view).ok_or("icon")?)
        .build()
        .map_err(|e| e.to_string())?;

    // A thread timer wakes the loop regularly to pick up menu events and refresh the state.
    unsafe { SetTimer(ptr::null_mut(), 0, 250, None) };
    let mut last_refresh = Instant::now();
    let mut msg: MSG = unsafe { mem::zeroed() };
    while unsafe { GetMessageW(&mut msg, ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let mut acted = false;
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(&action) = actions.get(&event.id) {
                perform(&rt, action);
                acted = true;
            }
        }
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                perform(&rt, Action::OpenDashboard);
            }
        }
        if acted || last_refresh.elapsed() >= Duration::from_secs(2) {
            last_refresh = Instant::now();
            let new_view = View::build(&rt);
            if acted || new_view != view {
                view = new_view;
                actions = apply(&tray, &view);
            }
        }
    }
    Ok(())
}
