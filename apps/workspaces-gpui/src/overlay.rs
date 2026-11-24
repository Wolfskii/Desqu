//! The note that shows the desktop's name for a moment when you switch desktops: a small dark panel in the
//! middle of the screen that never takes the focus and lets clicks through.

use gpui::{div, prelude::*, px, rgba, size, AnyWindowHandle, App, Bounds, Context, Global, IntoElement, Render, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// How long the note stays.
const SHOWN_FOR: std::time::Duration = std::time::Duration::from_millis(1300);

#[derive(Default)]
struct Current(Option<AnyWindowHandle>);
impl Global for Current {}

struct Note {
    name: String,
}

impl Render for Note {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(14.))
            .border_1()
            .border_color(rgba(0xffffff30))
            .bg(rgba(0x14171ee8))
            .text_color(rgba(0xffffffff))
            .text_size(px(28.))
            .child(self.name.clone())
    }
}

fn hwnd_of(window: &Window) -> Option<isize> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

/// Shows the note (replacing one that is still up) and removes it after a moment.
pub fn show(cx: &mut App, name: String) {
    if let Some(Current(Some(old))) = cx.try_global::<Current>().map(|c| Current(c.0)) {
        let _ = old.update(cx, |_, window, _| window.remove_window());
    }
    let bounds = Bounds::centered(None, size(px(380.), px(96.)), cx);
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: false,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        move |window, cx| {
            if let Some(hwnd) = hwnd_of(window) {
                workspaces_platform::make_overlay(hwnd);
            }
            cx.new(|_| Note { name })
        },
    );
    let Ok(handle) = opened else { return };
    let handle: AnyWindowHandle = handle.into();
    cx.set_global(Current(Some(handle)));
    cx.spawn(async move |cx| {
        cx.background_executor().timer(SHOWN_FOR).await;
        let _ = cx.update(|cx| {
            // Only the note that is still the current one.
            if cx.try_global::<Current>().and_then(|c| c.0) == Some(handle) {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
                cx.set_global(Current(None));
            }
        });
    })
    .detach();
}
