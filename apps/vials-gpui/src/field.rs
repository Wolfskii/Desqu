//! A single-line text field: typing, Backspace/Delete, arrows, Home/End and Ctrl+V. GPUI ships no
//! text input, so this is the small one the rules window needs.

use gpui::{div, prelude::*, px, rgba, Context, FocusHandle, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, Render, SharedString, Window};

pub struct TextField {
    text: String,
    /// Caret position in characters.
    cursor: usize,
    focus: FocusHandle,
    placeholder: SharedString,
}

/// Editing actions on a string with a caret; pure, so the rules of the editor are tested.
pub fn insert(text: &mut String, cursor: &mut usize, typed: &str) {
    let at = byte_index(text, *cursor);
    text.insert_str(at, typed);
    *cursor += typed.chars().count();
}

pub fn backspace(text: &mut String, cursor: &mut usize) {
    if *cursor > 0 {
        let at = byte_index(text, *cursor - 1);
        text.remove(at);
        *cursor -= 1;
    }
}

pub fn delete(text: &mut String, cursor: usize) {
    if cursor < text.chars().count() {
        let at = byte_index(text, cursor);
        text.remove(at);
    }
}

fn byte_index(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(i, _)| i)
}

impl TextField {
    pub fn new(placeholder: &str, cx: &mut Context<Self>) -> Self {
        Self { text: String::new(), cursor: 0, focus: cx.focus_handle(), placeholder: placeholder.to_string().into() }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn set_placeholder(&mut self, placeholder: &str, cx: &mut Context<Self>) {
        self.placeholder = placeholder.to_string().into();
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.text.clear();
        self.cursor = 0;
        cx.notify();
    }

    fn on_key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) {
        let k = &e.keystroke;
        let len = self.text.chars().count();
        match k.key.as_str() {
            "backspace" => backspace(&mut self.text, &mut self.cursor),
            "delete" => delete(&mut self.text, self.cursor),
            "left" => self.cursor = self.cursor.saturating_sub(1),
            "right" => self.cursor = (self.cursor + 1).min(len),
            "home" => self.cursor = 0,
            "end" => self.cursor = len,
            "v" if k.modifiers.control => {
                if let Some(pasted) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    insert(&mut self.text, &mut self.cursor, &pasted.replace(['\r', '\n'], " "));
                }
            }
            _ => match k.key_char.as_ref().filter(|_| !k.modifiers.control && !k.modifiers.alt) {
                Some(typed) => insert(&mut self.text, &mut self.cursor, typed),
                None => return,
            },
        }
        cx.notify();
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        let shown = if focused {
            let at = byte_index(&self.text, self.cursor);
            format!("{}|{}", &self.text[..at], &self.text[at..])
        } else {
            self.text.clone()
        };
        div()
            .id("field")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| this.on_key(e, cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus);
                    this.cursor = this.text.chars().count();
                    cx.notify();
                }),
            )
            .h(px(30.))
            .w_full()
            .px_2()
            .flex()
            .items_center()
            .rounded(px(6.))
            .border_1()
            .border_color(rgba(if focused { 0xaad2ffd9 } else { 0xffffff30 }))
            .bg(rgba(0x00000040))
            .text_sm()
            .when(self.text.is_empty() && !focused, |d| d.text_color(rgba(0xffffff60)).child(self.placeholder.clone()))
            .when(!self.text.is_empty() || focused, |d| d.child(shown))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_inserts_at_the_caret_and_moves_it() {
        let (mut t, mut c) = (String::from("ac"), 1);
        insert(&mut t, &mut c, "b");
        assert_eq!((t.as_str(), c), ("abc", 2));
        insert(&mut t, &mut c, "åä");
        assert_eq!((t.as_str(), c), ("abåäc", 4), "characters, not bytes");
    }

    #[test]
    fn backspace_and_delete_remove_around_the_caret() {
        let (mut t, mut c) = (String::from("åbc"), 1);
        backspace(&mut t, &mut c);
        assert_eq!((t.as_str(), c), ("bc", 0));
        backspace(&mut t, &mut c);
        assert_eq!(t, "bc", "nothing before the caret");
        delete(&mut t, 0);
        assert_eq!(t, "c");
        delete(&mut t, 5);
        assert_eq!(t, "c", "nothing after the caret");
    }
}
