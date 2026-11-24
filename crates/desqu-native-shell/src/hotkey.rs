//! One global hotkey for an app: written as text (`ctrl+alt+z`, `win+shift+space`), registered with
//! Windows on a thread of its own, and given back when the [`Hotkey`] is dropped.

/// A key combination: modifier bits (as Windows' `MOD_*`: Alt 1, Ctrl 2, Shift 4, Win 8) and a virtual key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo {
    pub modifiers: u32,
    pub key: u32,
}

const ALT: u32 = 1;
const CTRL: u32 = 2;
const SHIFT: u32 = 4;
const WIN: u32 = 8;

/// `ctrl+alt+z` as a combination, or `None` when the text is not one (no key, two keys, an unknown name, or
/// no modifier at all: a bare letter would take that letter from every program).
pub fn parse(text: &str) -> Option<Combo> {
    let mut modifiers = 0;
    let mut key = None;
    for part in text.split('+').map(|p| p.trim().to_ascii_lowercase()).filter(|p| !p.is_empty()) {
        match part.as_str() {
            "ctrl" | "control" => modifiers |= CTRL,
            "alt" => modifiers |= ALT,
            "shift" => modifiers |= SHIFT,
            "win" | "windows" | "super" => modifiers |= WIN,
            other => {
                if key.replace(key_code(other)?).is_some() {
                    return None;
                }
            }
        }
    }
    (modifiers != 0).then_some(Combo { modifiers, key: key? })
}

/// The virtual-key code of a key name: a letter, a digit, F1 to F12 or a few named keys.
fn key_code(name: &str) -> Option<u32> {
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(c.to_ascii_uppercase() as u32),
        (Some('f'), Some(_)) => name[1..].parse::<u32>().ok().filter(|n| (1..=12).contains(n)).map(|n| 0x6F + n),
        _ => match name {
            "space" => Some(0x20),
            "left" => Some(0x25),
            "up" => Some(0x26),
            "right" => Some(0x27),
            "down" => Some(0x28),
            "tab" => Some(0x09),
            "enter" | "return" => Some(0x0D),
            "esc" | "escape" => Some(0x1B),
            _ => None,
        },
    }
}

/// A registered global hotkey. Dropping it releases the key.
#[cfg(windows)]
pub struct Hotkey {
    thread_id: u32,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(windows)]
impl Hotkey {
    /// Registers `text` and calls `on_press` (on a thread of its own) each time it is pressed. Fails when
    /// the text is not a combination or another program already holds it.
    pub fn register(text: &str, on_press: Box<dyn Fn() + Send + 'static>) -> Result<Self, String> {
        use windows::Win32::System::Threading::GetCurrentThreadId;
        use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT};
        use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};
        let combo = parse(text).ok_or_else(|| format!("\"{text}\" is not a key combination like ctrl+alt+z"))?;
        let shown = text.to_string();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
        let thread = std::thread::Builder::new()
            .name("desqu-hotkey".into())
            .spawn(move || {
                // SAFETY: registered and released on this thread, which owns the message queue.
                unsafe {
                    if RegisterHotKey(None, 1, HOT_KEY_MODIFIERS(combo.modifiers) | MOD_NOREPEAT, combo.key).is_err() {
                        let _ = ready_tx.send(Err(format!("Windows would not give {shown} (another program has it)")));
                        return;
                    }
                    let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        if msg.message == WM_HOTKEY {
                            on_press();
                        }
                    }
                    let _ = UnregisterHotKey(None, 1);
                }
            })
            .map_err(|e| e.to_string())?;
        let thread_id = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Self { thread_id, thread: Some(thread) })
    }
}

#[cfg(windows)]
impl Drop for Hotkey {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
        // SAFETY: posts WM_QUIT to the hotkey thread so its message loop ends.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinations_are_read_from_text() {
        assert_eq!(parse("ctrl+alt+z"), Some(Combo { modifiers: CTRL | ALT, key: b'Z' as u32 }));
        assert_eq!(parse(" Win + Shift + Space "), Some(Combo { modifiers: WIN | SHIFT, key: 0x20 }));
        assert_eq!(parse("ctrl+F5"), Some(Combo { modifiers: CTRL, key: 0x74 }));
        assert_eq!(parse("win+ctrl+3"), Some(Combo { modifiers: WIN | CTRL, key: b'3' as u32 }));
    }

    #[test]
    fn text_that_is_not_a_combination_is_refused() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("z"), None, "a bare letter would be taken from every program");
        assert_eq!(parse("ctrl+"), None, "no key");
        assert_eq!(parse("ctrl+a+b"), None, "two keys");
        assert_eq!(parse("ctrl+banana"), None);
        assert_eq!(parse("ctrl+f13"), None);
    }
}
