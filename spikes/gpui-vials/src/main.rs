//! Spike: the Vials window rebuilt in GPUI, only to measure it against the Tauri + Svelte version.
//! Same three vials (position, size, 12 items each), a blurred transparent window, rounded corners.
//! No engine, no desktop hooks, no tray: this is the UI layer only.

use gpui::{
    div, hsla, point, prelude::*, px, rgba, size, white, App, Application, Bounds, Context, Hsla,
    SharedString, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowOptions,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

struct VialView {
    name: SharedString,
    items: Vec<(SharedString, Hsla)>,
}

impl Render for VialView {
    fn render(&mut self, _w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .rounded(px(12.))
            .border_1()
            .border_color(rgba(0xffffff44))
            .bg(rgba(0x181c2633))
            .text_color(white())
            .flex()
            .flex_col()
            .child(div().h(px(34.)).px_3().flex().items_center().text_sm().child(self.name.clone()))
            .child(
                div()
                    .flex_1()
                    .p_2()
                    .flex()
                    .flex_wrap()
                    .content_start()
                    .gap_1()
                    .children(self.items.iter().map(|(label, color)| {
                        div()
                            .w(px(84.))
                            .py_2()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_1()
                            .child(div().size(px(32.)).rounded(px(6.)).bg(*color))
                            .child(div().text_xs().child(label.clone()))
                    })),
            )
    }
}

#[cfg(windows)]
fn round_corners(window: &Window) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
    if let Ok(h) = HasWindowHandle::window_handle(window) {
        if let RawWindowHandle::Win32(w) = h.as_raw() {
            let hwnd = HWND(w.hwnd.get() as *mut std::ffi::c_void);
            let pref = DWMWCP_ROUND;
            // SAFETY: our own window; the value is a plain i32 enum.
            unsafe {
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &pref as *const _ as *const std::ffi::c_void,
                    std::mem::size_of_val(&pref) as u32,
                );
            }
        }
    }
}

fn main() {
    // (name, x, y, width, height) as fractions of the primary monitor: identical to the Tauri run.
    let vials = [("Vial 1", 0.10, 0.10, 0.16, 0.34), ("Vial 2", 0.30, 0.20, 0.16, 0.34), ("Vial 3", 0.50, 0.30, 0.16, 0.34)];
    Application::new().run(move |cx: &mut App| {
        let display = cx.primary_display().map(|d| d.bounds()).unwrap_or(Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))));
        for (name, fx, fy, fw, fh) in vials {
            let bounds = Bounds::new(
                point(display.origin.x + display.size.width * fx, display.origin.y + display.size.height * fy),
                size(display.size.width * fw, display.size.height * fh),
            );
            let items: Vec<(SharedString, Hsla)> = (0..12)
                .map(|i| (format!("Item {}", i + 1).into(), hsla(i as f32 / 12.0, 0.6, 0.55, 1.0)))
                .collect();
            let opened = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions { title: Some(name.into()), appears_transparent: true, ..Default::default() }),
                    focus: false,
                    is_resizable: true,
                    window_background: WindowBackgroundAppearance::Blurred,
                    ..Default::default()
                },
                |_, cx| cx.new(|_| VialView { name: name.into(), items }),
            );
            if let Ok(handle) = opened {
                let _ = handle.update(cx, |_, window, _| {
                    #[cfg(windows)]
                    round_corners(window);
                });
            }
        }
    });
}
