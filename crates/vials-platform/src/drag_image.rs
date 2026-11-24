//! The picture that follows the cursor while several items are dragged: the first icons fanned out
//! and a count, like Explorer's own. Pure pixels (premultiplied 0xAARRGGBB), so it is tested here;
//! the windows glue that fetches the icons and hands the picture to the drag helper is in `windows.rs`.

/// The size of one icon.
pub const ICON: usize = 48;
/// How many icons are fanned out; the rest only count.
const FAN: usize = 4;
/// How far each further icon sits to the right and below the previous one.
const STEP: usize = 8;
const BADGE_RADIUS: i32 = 10;
const BADGE: u32 = 0xFF1F6FEB;

pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

/// `icons` are ICON x ICON premultiplied pixels, the first one is drawn on top. `total` is how many
/// items are dragged (it can be more than the icons given); with more than one a count is drawn.
pub fn compose(icons: &[Vec<u32>], total: usize) -> Picture {
    let shown = icons.len().clamp(1, FAN);
    let side = ICON + STEP * (shown - 1);
    let mut pixels = vec![0u32; side * side];
    // Back to front, so the first icon ends on top.
    for (i, icon) in icons.iter().take(shown).enumerate().rev() {
        let (ox, oy) = (i * STEP, i * STEP);
        for y in 0..ICON {
            for x in 0..ICON {
                let Some(&src) = icon.get(y * ICON + x) else { continue };
                let at = (oy + y) * side + ox + x;
                pixels[at] = over(src, pixels[at]);
            }
        }
    }
    if total > 1 {
        badge(&mut pixels, side, total);
    }
    Picture { width: side, height: side, pixels }
}

/// `src` over `dst`, both premultiplied.
fn over(src: u32, dst: u32) -> u32 {
    let inverse = 255 - (src >> 24);
    let channel = |shift: u32| {
        let s = (src >> shift) & 0xFF;
        let d = (dst >> shift) & 0xFF;
        (s + d * inverse / 255).min(255) << shift
    };
    channel(24) | channel(16) | channel(8) | channel(0)
}

/// A filled circle in the bottom-right corner with the count in it.
fn badge(pixels: &mut [u32], side: usize, total: usize) {
    let (cx, cy) = (side as i32 - BADGE_RADIUS - 1, side as i32 - BADGE_RADIUS - 1);
    for y in 0..side as i32 {
        for x in 0..side as i32 {
            let (dx, dy) = (x - cx, y - cy);
            if dx * dx + dy * dy <= BADGE_RADIUS * BADGE_RADIUS {
                pixels[y as usize * side + x as usize] = BADGE;
            }
        }
    }
    let text = total.min(99).to_string();
    // Each digit is 3 x 5 cells drawn 2 px per cell, with a 2 px gap.
    let width = text.len() as i32 * 8 - 2;
    let (mut left, top) = (cx - width / 2, cy - 5);
    for digit in text.bytes().map(|b| (b - b'0') as usize) {
        for (row, bits) in DIGITS[digit].iter().enumerate() {
            for col in 0..3 {
                if bits & (0b100 >> col) != 0 {
                    for (px, py) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (x, y) = (left + col * 2 + px, top + row as i32 * 2 + py);
                        if (0..side as i32).contains(&x) && (0..side as i32).contains(&y) {
                            pixels[y as usize * side + x as usize] = 0xFFFFFFFF;
                        }
                    }
                }
            }
        }
        left += 8;
    }
}

const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(color: u32) -> Vec<u32> {
        vec![color; ICON * ICON]
    }

    #[test]
    fn one_icon_is_drawn_as_it_is_without_a_count() {
        let p = compose(&[solid(0xFF112233)], 1);
        assert_eq!((p.width, p.height), (ICON, ICON));
        assert!(p.pixels.iter().all(|&c| c == 0xFF112233));
    }

    #[test]
    fn several_icons_fan_out_with_the_first_on_top_and_a_count() {
        let p = compose(&[solid(0xFFFF0000), solid(0xFF00FF00), solid(0xFF0000FF)], 3);
        assert_eq!(p.width, ICON + 2 * STEP);
        assert_eq!(p.pixels[0], 0xFFFF0000, "the first icon covers the top-left corner");
        // Right of the second icon, above the badge: only the third icon reaches there.
        assert_eq!(p.pixels[30 * p.width + ICON + 2 * STEP - 2], 0xFF0000FF, "the third icon shows where only it reaches");
        assert!(p.pixels.contains(&BADGE), "the count badge is drawn");
        assert!(p.pixels.contains(&0xFFFFFFFF), "with the count in white");
    }

    #[test]
    fn a_transparent_icon_lets_the_one_behind_show() {
        let p = compose(&[solid(0x00000000), solid(0xFF00FF00)], 2);
        assert_eq!(p.pixels[STEP * p.width + STEP], 0xFF00FF00);
    }

    #[test]
    fn more_icons_than_the_fan_holds_are_only_counted() {
        let icons: Vec<Vec<u32>> = (0..6).map(|_| solid(0xFF445566)).collect();
        assert_eq!(compose(&icons, 6).width, ICON + 3 * STEP);
    }
}
