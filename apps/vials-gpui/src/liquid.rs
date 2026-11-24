//! Liquid glass. Windows can blur what is behind a window but cannot bend it, and GPUI exposes no
//! shaders, so the glass is computed here: a vial sits on the desktop below every other window, so what
//! is behind it is the wallpaper. This reads the wallpaper, and for the rectangle of a vial computes
//! (per pixel, on the CPU) what a glass slab would show: the picture bent at the rim like a lens, a
//! little colour fringing, soft blur inside, a tint for readable labels, and rim light.
//!
//! Everything but `load` is pure and unit tested.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use image::{imageops, Rgba, RgbaImage};

/// The wallpaper prepared for sampling.
pub struct Wallpaper {
    /// The picture, at most `SHARP_MAX` px on its long side.
    sharp: RgbaImage,
    /// A heavily blurred small copy: what is seen through the middle of the glass.
    soft: RgbaImage,
    /// Size of the original file (the layout is computed with it).
    original: (u32, u32),
    /// Windows' `WallpaperStyle`: 0 center, 2 stretch, 6 fit, 10 fill, 22 span.
    style: u32,
    tile: bool,
    modified: Option<SystemTime>,
}

const SHARP_MAX: u32 = 1600;

/// Where Windows draws a wallpaper of `image` px on a monitor of `monitor` px:
/// `(left, top, width, height)` in monitor pixels.
pub fn layout(style: u32, tile: bool, monitor: (f32, f32), image: (f32, f32)) -> (f32, f32, f32, f32) {
    let ((mw, mh), (w, h)) = (monitor, image);
    if tile {
        return (0.0, 0.0, w, h);
    }
    match style {
        2 => (0.0, 0.0, mw, mh),
        0 => ((mw - w) / 2.0, (mh - h) / 2.0, w, h),
        _ => {
            // 6 = fit (whole picture visible); fill and span cover the monitor.
            let s = if style == 6 { (mw / w).min(mh / h) } else { (mw / w).max(mh / h) };
            ((mw - w * s) / 2.0, (mh - h * s) / 2.0, w * s, h * s)
        }
    }
}

/// Bilinear sample at (`u`, `v`) in 0..1 of the picture. Outside it: black, or wrapped when tiled.
fn sample(img: &RgbaImage, mut u: f32, mut v: f32, tile: bool) -> [f32; 3] {
    if tile {
        u = u.rem_euclid(1.0);
        v = v.rem_euclid(1.0);
    } else if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
        return [0.0; 3];
    }
    let (w, h) = (img.width() as f32, img.height() as f32);
    let (x, y) = ((u * w - 0.5).clamp(0.0, w - 1.0), (v * h - 0.5).clamp(0.0, h - 1.0));
    let (x0, y0) = (x.floor() as u32, y.floor() as u32);
    let (x1, y1) = ((x0 + 1).min(img.width() - 1), (y0 + 1).min(img.height() - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let px = |x: u32, y: u32| img.get_pixel(x, y).0;
    let mut out = [0.0; 3];
    for (c, o) in out.iter_mut().enumerate() {
        let top = f32::from(px(x0, y0)[c]) * (1.0 - fx) + f32::from(px(x1, y0)[c]) * fx;
        let bottom = f32::from(px(x0, y1)[c]) * (1.0 - fx) + f32::from(px(x1, y1)[c]) * fx;
        *o = (top * (1.0 - fy) + bottom * fy) / 255.0;
    }
    out
}

impl Wallpaper {
    /// Builds the sampling pictures from a decoded wallpaper.
    pub fn from_image(original: RgbaImage, style: u32, tile: bool, modified: Option<SystemTime>) -> Self {
        let size = (original.width(), original.height());
        let k = (SHARP_MAX as f32 / size.0.max(size.1) as f32).min(1.0);
        let sharp = if k < 1.0 {
            imageops::resize(&original, (size.0 as f32 * k) as u32, (size.1 as f32 * k) as u32, imageops::FilterType::Triangle)
        } else {
            original
        };
        let small = imageops::resize(&sharp, (sharp.width() / 8).max(2), (sharp.height() / 8).max(2), imageops::FilterType::Triangle);
        let soft = imageops::blur(&small, 3.0);
        Self { sharp, soft, original: size, style, tile, modified }
    }

    /// The average colour of the wallpaper (`0xRRGGBB`), from its small blurred copy.
    pub fn average_color(&self) -> Option<u32> {
        vials_core::chameleon::average(self.soft.pixels().map(|p| [p.0[0], p.0[1], p.0[2]]))
    }

    /// Reads the current wallpaper from disk.
    pub fn load() -> Result<Self, String> {
        let (path, style, tile) = vials_platform::wallpaper()?;
        let bytes = std::fs::read(&path).map_err(|e| format!("cannot read the wallpaper: {e}"))?;
        let decoded = image::load_from_memory(&bytes).map_err(|e| format!("cannot decode the wallpaper: {e}"))?.into_rgba8();
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        Ok(Self::from_image(decoded, style, tile, modified))
    }
}

/// The loaded wallpaper and when it was last checked.
type Cache = Mutex<(Option<Arc<Wallpaper>>, Option<Instant>)>;

static CURRENT: OnceLock<Cache> = OnceLock::new();

/// The wallpaper, loaded on first use and re-read when the file changed (checked at most every 5 s).
/// Slow the first time: call it off the UI thread.
pub fn current() -> Option<Arc<Wallpaper>> {
    let cell = CURRENT.get_or_init(|| Mutex::new((None, None)));
    let mut state = cell.lock().unwrap_or_else(|e| e.into_inner());
    let fresh = state.1.is_some_and(|at| at.elapsed() < Duration::from_secs(5));
    if state.0.is_some() && fresh {
        return state.0.clone();
    }
    state.1 = Some(Instant::now());
    if let (Some(have), Ok((path, _, _))) = (&state.0, vials_platform::wallpaper()) {
        let now = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if now == have.modified {
            return state.0.clone();
        }
    }
    match Wallpaper::load() {
        Ok(w) => state.0 = Some(Arc::new(w)),
        Err(e) => eprintln!("liquid glass: {e}"),
    }
    state.0.clone()
}

/// What to draw: the vial's rectangle and the monitor it is on, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    pub width: u32,
    pub height: u32,
    /// Top-left of the window on its monitor.
    pub window: (i32, i32),
    pub monitor: (u32, u32),
    pub radius: f32,
    /// Width of the rim that bends the picture.
    pub bezel: f32,
    /// 0 (clear) to 1 (dark).
    pub tint: f32,
    /// Display scale (1.0 = 100 %).
    pub dpr: f32,
}

fn sd_box(qx: f32, qy: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let (ax, ay) = (qx.abs() - hx + r, qy.abs() - hy + r);
    (ax.max(0.0).powi(2) + ay.max(0.0).powi(2)).sqrt() + ax.max(ay).min(0.0) - r
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The glass for one vial, as BGRA pixels (the byte order GPUI draws) with straight alpha: opaque
/// inside the rounded rectangle, transparent outside it.
pub fn render(wp: &Wallpaper, p: &Params) -> RgbaImage {
    let (w, h) = (p.width.max(1), p.height.max(1));
    let (hx, hy) = (w as f32 / 2.0, h as f32 / 2.0);
    let radius = p.radius.min(hx).min(hy);
    let (ox, oy, dw, dh) = layout(wp.style, wp.tile, (p.monitor.0 as f32, p.monitor.1 as f32), (wp.original.0 as f32, wp.original.1 as f32));
    let uv = |x: f32, y: f32| ((p.window.0 as f32 + x - ox) / dw, (p.window.1 as f32 + y - oy) / dh);
    // The rim light comes from the top left; a fainter one from the opposite corner.
    let (lx, ly) = (-0.7f32 / 1.2207, -1.0f32 / 1.2207);

    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            let (qx, qy) = (cx - hx, cy - hy);
            let d = sd_box(qx, qy, hx, hy, radius);
            let cover = (0.5 - d).clamp(0.0, 1.0);
            if cover <= 0.0 {
                out.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                continue;
            }
            let depth = -d;
            let (gx, gy) = (sd_box(qx + 1.0, qy, hx, hy, radius) - sd_box(qx - 1.0, qy, hx, hy, radius), sd_box(qx, qy + 1.0, hx, hy, radius) - sd_box(qx, qy - 1.0, hx, hy, radius));
            let gl = (gx * gx + gy * gy).sqrt().max(1e-5);
            let (nx, ny) = (gx / gl, gy / gl);

            // Strongest bend right at the rim, easing to none at the inner edge of the bezel.
            let t = (depth / p.bezel).clamp(0.0, 1.0);
            let bend = (1.0 - t).powf(2.6);
            let push = bend * p.bezel * 0.95;

            let mut col = [0.0f32; 3];
            for (c, factor) in [0.92f32, 1.0, 1.10].into_iter().enumerate() {
                let (u, v) = uv(cx + nx * push * factor, cy + ny * push * factor);
                let sharp = sample(&wp.sharp, u, v, wp.tile)[c];
                let soft = sample(&wp.soft, u, v, wp.tile)[c];
                col[c] = mix(soft, sharp, bend);
            }

            // A little more colour, a slight lift, then the tint that keeps labels readable.
            let lum = col[0] * 0.299 + col[1] * 0.587 + col[2] * 0.114;
            let tint = [0.05, 0.07, 0.10];
            for (c, v) in col.iter_mut().enumerate() {
                *v = mix(lum, *v, 1.28) + 0.035;
                *v = mix(*v, tint[c], p.tint * 0.8);
            }

            let rim = smoothstep(2.2 * p.dpr, 0.0, depth);
            let spec = (nx * lx + ny * ly).max(0.0).powf(2.5) + 0.45 * (-(nx * lx + ny * ly)).max(0.0).powf(2.5);
            let glow = smoothstep(p.bezel, 0.0, depth) * 0.05 + (1.0 - smoothstep(0.0, 70.0 * p.dpr, (cx * cx + cy * cy).sqrt())) * 0.05;
            for v in col.iter_mut() {
                *v += rim * spec * 0.85 + glow;
            }

            let to_byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            // GPUI wants BGRA.
            out.put_pixel(x, y, Rgba([to_byte(col[2]), to_byte(col[1]), to_byte(col[0]), to_byte(cover)]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    /// Reads the real wallpaper (changes nothing): run by hand with `--ignored`.
    #[test]
    #[ignore = "reads the real wallpaper"]
    fn the_wallpaper_has_an_average_colour() {
        let wallpaper = current().expect("a wallpaper");
        let average = wallpaper.average_color().expect("an average");
        println!("average {average:06x}, panel {:06x}", vials_core::chameleon::panel_color(average));
        assert!(average <= 0xFFFFFF);
    }

    use super::*;

    fn flat(rgb: [u8; 3], w: u32, h: u32) -> Wallpaper {
        Wallpaper::from_image(RgbaImage::from_pixel(w, h, Rgba([rgb[0], rgb[1], rgb[2], 255])), 10, false, None)
    }

    fn params(w: u32, h: u32) -> Params {
        Params { width: w, height: h, window: (100, 100), monitor: (1920, 1080), radius: 14.0, bezel: 26.0, tint: 0.2, dpr: 1.0 }
    }

    #[test]
    fn fill_covers_the_monitor_fit_shows_it_all_and_stretch_distorts() {
        // A 1000 x 500 picture on a 1920 x 1080 monitor.
        let (ox, oy, w, h) = layout(10, false, (1920.0, 1080.0), (1000.0, 500.0));
        assert!((w - 2160.0).abs() < 0.01 && (h - 1080.0).abs() < 0.01, "fill scales to cover: {w}x{h}");
        assert!(ox < 0.0 && oy == 0.0, "and is centred");
        let (_, _, w, h) = layout(6, false, (1920.0, 1080.0), (1000.0, 500.0));
        assert!((w - 1920.0).abs() < 0.01 && (h - 960.0).abs() < 0.01, "fit keeps the whole picture: {w}x{h}");
        assert_eq!(layout(2, false, (1920.0, 1080.0), (10.0, 10.0)), (0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(layout(0, false, (100.0, 100.0), (40.0, 20.0)), (30.0, 40.0, 40.0, 20.0));
        assert_eq!(layout(10, true, (100.0, 100.0), (40.0, 20.0)), (0.0, 0.0, 40.0, 20.0), "tiled: its own size");
    }

    #[test]
    fn sampling_is_black_outside_unless_tiled() {
        let img = RgbaImage::from_pixel(4, 4, Rgba([255, 255, 255, 255]));
        assert_eq!(sample(&img, 1.5, 0.5, false), [0.0; 3]);
        assert!(sample(&img, 1.5, 0.5, true)[0] > 0.99);
        assert!(sample(&img, 0.5, 0.5, false)[1] > 0.99);
    }

    #[test]
    fn the_glass_is_transparent_outside_its_rounded_corners_and_opaque_inside() {
        let img = render(&flat([200, 120, 60], 1920, 1080), &params(200, 120));
        assert_eq!(img.get_pixel(0, 0).0[3], 0, "the corner is cut");
        assert_eq!(img.get_pixel(100, 60).0[3], 255);
        assert_eq!(img.get_pixel(100, 0).0[3], 255, "the middle of an edge is inside");
        assert_eq!((img.width(), img.height()), (200, 120));
    }

    #[test]
    fn the_tint_darkens_and_a_flat_picture_stays_flat_in_the_middle() {
        let wp = flat([200, 200, 200], 1920, 1080);
        let centre = |tint: f32| {
            let img = render(&wp, &Params { tint, ..params(300, 200) });
            img.get_pixel(150, 100).0
        };
        assert!(centre(0.9)[0] < centre(0.0)[0], "more tint is darker");
        // A flat picture shows the same colour all over the middle of the glass.
        let img = render(&wp, &Params { tint: 0.2, ..params(300, 200) });
        assert_eq!(img.get_pixel(150, 100), img.get_pixel(120, 90));
    }

    #[test]
    fn the_top_left_rim_catches_the_light() {
        let img = render(&flat([30, 30, 30], 1920, 1080), &params(300, 200));
        let rim = img.get_pixel(150, 1).0[0]; // top edge, facing the light
        let inside = img.get_pixel(150, 100).0[0];
        let bottom = img.get_pixel(150, 198).0[0];
        assert!(rim > inside + 20, "rim {rim} vs inside {inside}");
        assert!(rim > bottom, "lit more on the light's side");
    }

    #[test]
    fn the_rim_bends_the_picture() {
        // Left half black, right half white: near the vertical rim the glass shows content from outside.
        let mut half = RgbaImage::from_pixel(1920, 1080, Rgba([0, 0, 0, 255]));
        for y in 0..1080 {
            for x in 960..1920 {
                half.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let wp = Wallpaper::from_image(half, 2, false, None);
        // A vial whose left edge sits at x = 940: just left of the edge between black and white.
        let p = Params { window: (940, 300), tint: 0.0, ..params(200, 100) };
        let bent = render(&wp, &p);
        let flat_left = bent.get_pixel(3, 50).0[0];
        assert!(flat_left > 0, "the bent rim picked up white from the right side: {flat_left}");
    }
}

#[cfg(test)]
mod timing {
    use super::*;

    /// Rendering has to keep up with dragging a vial: a typical vial must take well under a frame or
    /// two. The limit is generous so the test does not flake on a busy machine.
    #[test]
    fn a_typical_vial_renders_fast_enough_to_follow_a_drag() {
        let wp = Wallpaper::from_image(RgbaImage::from_pixel(2560, 1440, Rgba([90, 60, 40, 255])), 10, false, None);
        let p = Params { width: 480, height: 320, window: (500, 300), monitor: (2560, 1440), radius: 14.0, bezel: 26.0, tint: 0.2, dpr: 1.0 };
        let start = Instant::now();
        for _ in 0..5 {
            let _ = render(&wp, &p);
        }
        let each = start.elapsed() / 5;
        eprintln!("liquid render 480x320: {each:?}");
        assert!(each < Duration::from_millis(400), "{each:?}");
    }
}
