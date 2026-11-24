//! A panel colour taken from the wallpaper ("chameleon"): the average of its pixels, made dark enough for the
//! text on it to be read. Pure.

/// The average colour (`0xRRGGBB`) of the pixels, or `None` for none.
pub fn average(pixels: impl IntoIterator<Item = [u8; 3]>) -> Option<u32> {
    let (mut sum, mut count) = ([0u64; 3], 0u64);
    for p in pixels {
        for (s, c) in sum.iter_mut().zip(p) {
            *s += u64::from(c);
        }
        count += 1;
    }
    (count > 0).then(|| {
        let [r, g, b] = sum.map(|s| (s / count) as u32);
        (r << 16) | (g << 8) | b
    })
}

/// The panel colour for a wallpaper whose average is `average`: each channel at half strength, so a bright
/// wallpaper gives a mid-dark panel and a dark one a very dark one.
pub fn panel_color(average: u32) -> u32 {
    let half = |shift: u32| ((average >> shift) & 0xFF) / 2;
    (half(16) << 16) | (half(8) << 8) | half(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_average_of_pixels_is_taken_per_channel() {
        assert_eq!(average([[10, 20, 30], [30, 40, 50]]), Some(0x14_1E_28));
        assert_eq!(average(std::iter::empty()), None);
        assert_eq!(average([[255, 255, 255]]), Some(0xFFFFFF));
    }

    #[test]
    fn the_panel_is_half_as_bright_as_the_wallpaper() {
        assert_eq!(panel_color(0xFF_80_00), 0x7F_40_00);
        assert_eq!(panel_color(0), 0);
    }
}
