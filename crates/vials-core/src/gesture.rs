//! The "drag a box on the empty desktop to make a vial" gesture. Pure.

use std::collections::HashSet;

use super::model::PixelRect;

/// Smaller boxes are ordinary clicks or slips of the mouse, not a request for a vial.
pub const MIN_WIDTH: i32 = 90;
pub const MIN_HEIGHT: i32 = 70;

/// The box dragged from `down` to `up` (in screen pixels), or `None` when it is too small.
pub fn marquee(down: (i32, i32), up: (i32, i32)) -> Option<PixelRect> {
    let rect = PixelRect {
        left: down.0.min(up.0),
        top: down.1.min(up.1),
        right: down.0.max(up.0),
        bottom: down.1.max(up.1),
    };
    (rect.width() >= MIN_WIDTH && rect.height() >= MIN_HEIGHT).then_some(rect)
}

/// The smallest `Vial N` not used yet.
pub fn next_name<'a>(existing: impl IntoIterator<Item = &'a str>) -> String {
    let taken: HashSet<&str> = existing.into_iter().collect();
    (1..).map(|n| format!("Vial {n}")).find(|name| !taken.contains(name.as_str())).expect("endless range")
}

/// The smallest `vial-N` id not used yet (ids become window labels and URL segments).
pub fn next_id<'a>(existing: impl IntoIterator<Item = &'a str>) -> String {
    let taken: HashSet<&str> = existing.into_iter().collect();
    (1..).map(|n| format!("vial-{n}")).find(|id| !taken.contains(id.as_str())).expect("endless range")
}

/// Where to put a popup of `size` that should open at `cursor`: at the cursor, moved back inside
/// `screen` when it would stick out.
pub fn popup_position(cursor: (i32, i32), size: (i32, i32), screen: &PixelRect) -> (i32, i32) {
    let x = cursor.0.min(screen.right - size.0).max(screen.left);
    let y = cursor.1.min(screen.bottom - size.1).max(screen.top);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dragged_box_becomes_a_rect_whichever_corner_it_started_from() {
        let r = marquee((500, 400), (200, 100)).unwrap();
        assert_eq!((r.left, r.top, r.right, r.bottom), (200, 100, 500, 400));
        let r = marquee((200, 400), (500, 100)).unwrap();
        assert_eq!((r.left, r.top, r.right, r.bottom), (200, 100, 500, 400));
    }

    #[test]
    fn clicks_and_slips_are_not_a_vial() {
        assert!(marquee((100, 100), (100, 100)).is_none());
        assert!(marquee((100, 100), (180, 400)).is_none(), "too narrow");
        assert!(marquee((100, 100), (400, 160)).is_none(), "too short");
        assert!(marquee((100, 100), (190, 170)).is_some(), "exactly the minimum");
    }

    #[test]
    fn names_and_ids_fill_the_first_gap() {
        assert_eq!(next_name([]), "Vial 1");
        assert_eq!(next_name(["Vial 1", "Vial 3", "Music"]), "Vial 2");
        assert_eq!(next_id(["vial-1", "vial-2"]), "vial-3");
        assert_eq!(next_id(["vial-2"]), "vial-1");
    }

    #[test]
    fn the_popup_stays_on_screen() {
        let screen = PixelRect { left: 0, top: 0, right: 1920, bottom: 1080 };
        assert_eq!(popup_position((100, 100), (200, 60), &screen), (100, 100));
        assert_eq!(popup_position((1900, 1070), (200, 60), &screen), (1720, 1020));
        let second = PixelRect { left: -1920, top: 0, right: 0, bottom: 1080 };
        assert_eq!(popup_position((-10, 500), (200, 60), &second), (-200, 500));
    }
}
