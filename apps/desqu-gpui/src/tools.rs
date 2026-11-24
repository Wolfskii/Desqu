//! The small tools that live in Desqu itself (they have no program of their own): always on top, keep awake,
//! and light and dark mode on a schedule. Each is turned on and off on the Tools page. What decides things
//! is pure and tested; the rest only connects it to Windows through `desqu_native_shell::windowtools`.

use std::collections::HashSet;
use std::sync::mpsc::Sender;

use desqu_native_shell::hotkey::Hotkey;
use desqu_native_shell::windowtools as win;

use crate::modules::Settings;

// ------------------------------------------------------------------------------------------- the schedule

/// Whether the light theme is wanted at `minute` (since midnight) when the light one starts at `light_from` and the
/// dark one at `dark_from`. The two can be in either order, and the day wraps at midnight.
pub fn light_at(minute: u16, light_from: u16, dark_from: u16) -> bool {
    if light_from == dark_from {
        return true;
    }
    if light_from < dark_from {
        minute >= light_from && minute < dark_from
    } else {
        // Light from the evening to the morning (an odd choice, but it is the user's).
        minute >= light_from || minute < dark_from
    }
}

/// `7:30` or `19.05` as minutes since midnight.
#[cfg(test)]
pub fn parse_time(text: &str) -> Option<u16> {
    let (h, m) = text.trim().split_once([':', '.'])?;
    let (h, m): (u16, u16) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

pub fn clock(minutes: u16) -> String {
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

// ----------------------------------------------------------------------------------------------- the runtime

/// What a tool asks the window to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolEvent {
    /// The always-on-top hotkey was pressed.
    TogglePin,
}

/// The tools that are on, and what they hold on to.
#[derive(Default)]
pub struct Tools {
    pin_key: Option<(String, Hotkey)>,
    /// Windows that were pinned by the hotkey (so they can be let go when the tool is turned off).
    pinned: HashSet<isize>,
    awake: Option<(bool, win::KeepAwake)>,
    /// The last theme the schedule put on, so the user's own change in between is not undone every minute.
    last_scheduled: Option<bool>,
}

impl Tools {
    /// Brings the running tools in line with the settings. Cheap to call every second.
    pub fn apply(&mut self, s: &Settings, events: &Sender<ToolEvent>) {
        // Always on top: a hotkey.
        let wanted = s.tool_pin_hotkey.clone();
        if self.pin_key.as_ref().map_or("", |(t, _)| t.as_str()) != wanted {
            self.pin_key = None;
            if !wanted.is_empty() {
                let tx = events.clone();
                match Hotkey::register(&wanted, Box::new(move || {
                    let _ = tx.send(ToolEvent::TogglePin);
                })) {
                    Ok(h) => self.pin_key = Some((wanted, h)),
                    Err(e) => eprintln!("{e}"),
                }
            }
            if self.pin_key.is_none() {
                self.unpin_all();
            }
        }
        // Keep awake.
        match (s.tool_keep_awake, self.awake.as_ref().map(|a| a.0)) {
            (true, Some(display)) if display == s.tool_keep_display => {}
            (true, _) => self.awake = Some((s.tool_keep_display, win::KeepAwake::start(s.tool_keep_display))),
            (false, _) => self.awake = None,
        }
        // Light and dark by the clock: put it on when the wanted theme changes, so a change by hand stays until then.
        if s.tool_auto_dark {
            let want = light_at(win::minute_of_day_now(), s.light_from(), s.dark_from());
            if self.last_scheduled != Some(want) {
                self.last_scheduled = Some(want);
                if win::apps_use_light() != Some(want) {
                    if let Err(e) = win::set_light(want) {
                        eprintln!("{e}");
                    }
                }
            }
        } else {
            self.last_scheduled = None;
        }
    }

    /// The always-on-top key was pressed: the window in front is pinned or let go.
    pub fn toggle_pin(&mut self) {
        let Some(window) = win::foreground() else { return };
        let on = !win::is_topmost(window);
        win::set_topmost(window, on);
        // A thin accent border says which window is pinned (Windows 11; elsewhere nothing shows).
        win::set_border_color(window, on.then_some(0x4fa3ff));
        if on {
            self.pinned.insert(window);
        } else {
            self.pinned.remove(&window);
        }
    }

    fn unpin_all(&mut self) {
        for window in self.pinned.drain() {
            win::set_topmost(window, false);
            win::set_border_color(window, None);
        }
    }
}

impl Drop for Tools {
    fn drop(&mut self) {
        self.unpin_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schedule_gives_light_by_day_and_dark_by_night_and_wraps_at_midnight() {
        let (light, dark) = (7 * 60, 19 * 60);
        assert!(light_at(7 * 60, light, dark) && light_at(12 * 60, light, dark) && light_at(19 * 60 - 1, light, dark));
        assert!(!light_at(19 * 60, light, dark) && !light_at(3 * 60, light, dark) && !light_at(0, light, dark));
        // Light starting in the evening and dark in the morning.
        assert!(light_at(22 * 60, 20 * 60, 6 * 60) && light_at(60, 20 * 60, 6 * 60) && !light_at(12 * 60, 20 * 60, 6 * 60));
        assert!(light_at(5, 100, 100), "the same time twice means no schedule: light");
    }

    #[test]
    fn a_time_is_read_with_a_colon_or_a_point_and_refused_when_it_is_not_one() {
        assert_eq!(parse_time("7:30"), Some(450));
        assert_eq!(parse_time(" 19.05 "), Some(19 * 60 + 5));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(parse_time("7:60"), None);
        assert_eq!(parse_time("seven"), None);
        assert_eq!(clock(19 * 60 + 5), "19:05");
    }

    #[test]
    fn keeping_awake_starts_and_ends_without_leaving_anything_held() {
        let awake = win::KeepAwake::start(false);
        drop(awake);
        assert!(win::apps_use_light().is_some(), "the theme setting can be read");
    }
}
