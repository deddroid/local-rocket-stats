//! Linux tray via `ksni` (StatusNotifierItem over D-Bus: KDE Plasma, GNOME with AppIndicator, …).

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ksni::blocking::TrayMethods;
use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Icon, MenuItem, ToolTip};

use super::{Action, View, perform};
use crate::i18n::{T, t};
use crate::runtime::Runtime;
use crate::settings::Lang;

struct Tray {
    rt: Arc<Runtime>,
    view: View,
}

fn item(label: impl Into<String>, action: Action) -> MenuItem<Tray> {
    StandardItem {
        label: label.into(),
        activate: Box::new(move |tray: &mut Tray| tray.act(action)),
        ..Default::default()
    }
    .into()
}

fn check(label: &str, checked: bool, action: Action) -> MenuItem<Tray> {
    CheckmarkItem {
        label: label.into(),
        checked,
        activate: Box::new(move |tray: &mut Tray| tray.act(action)),
        ..Default::default()
    }
    .into()
}

impl Tray {
    fn act(&mut self, action: Action) {
        perform(&self.rt, action);
        self.view = View::build(&self.rt);
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "local-rocket-stats".into()
    }

    fn title(&self) -> String {
        "Local Rocket Stats".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        [22u32, 32, 48, 64]
            .iter()
            .map(|&s| {
                // ksni wants ARGB.
                let mut data = super::icon_rgba(s, &self.view);
                for px in data.chunks_exact_mut(4) {
                    px.rotate_right(1);
                }
                Icon {
                    width: s as i32,
                    height: s as i32,
                    data,
                }
            })
            .collect()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "Local Rocket Stats".into(),
            description: self.view.state.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.act(Action::OpenDashboard);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let v = &self.view;
        let l = v.lang;
        let mut items: Vec<MenuItem<Self>> = vec![
            StandardItem {
                label: v.state.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            item(t(l, T::OpenDashboard), Action::OpenDashboard),
        ];
        if !v.stats_api_on {
            items.push(item(t(l, T::EnableStatsApi), Action::EnableStatsApi));
        }
        items.push(check(t(l, T::PhoneAccess), v.lan, Action::ToggleLan));
        if let Some(url) = &v.phone_url {
            items.push(
                StandardItem {
                    label: format!("{}: {url}", t(l, T::Phone)),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(check(
            t(l, T::StartAtLogin),
            v.autostart,
            Action::ToggleAutostart,
        ));
        let selected = if v.lang_auto {
            0
        } else if l == Lang::It {
            2
        } else {
            1
        };
        items.push(
            SubMenu {
                label: t(l, T::Language).into(),
                submenu: vec![
                    RadioGroup {
                        selected,
                        select: Box::new(|tray: &mut Self, i| {
                            tray.act(Action::SetLang(
                                [None, Some(Lang::En), Some(Lang::It)][i.min(2)],
                            ))
                        }),
                        options: vec![
                            RadioItem {
                                label: t(l, T::LangAuto).into(),
                                ..Default::default()
                            },
                            RadioItem {
                                label: "English".into(),
                                ..Default::default()
                            },
                            RadioItem {
                                label: "Italiano".into(),
                                ..Default::default()
                            },
                        ],
                    }
                    .into(),
                ],
                ..Default::default()
            }
            .into(),
        );
        items.push(item(t(l, T::OpenLog), Action::OpenLog));
        items.push(MenuItem::Separator);
        items.push(item(t(l, T::Quit), Action::Quit));
        items
    }
}

/// Runs the tray until "Quit". Returns an error if there is no tray on this desktop.
pub fn run(rt: Arc<Runtime>) -> Result<(), String> {
    let view = View::build(&rt);
    let handle = Tray {
        rt: Arc::clone(&rt),
        view,
    }
    .spawn()
    .map_err(|e| e.to_string())?;
    loop {
        thread::sleep(Duration::from_secs(2));
        let view = View::build(&rt);
        let changed = handle.update(|tray| {
            let changed = tray.view != view;
            if changed {
                tray.view = view;
            }
            changed
        });
        if changed.is_none() {
            return Ok(());
        }
    }
}
