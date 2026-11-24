//! How a vial looks: the window material and the colours of the panel, per theme. Pure data, so the
//! choices are unit tested. Colours are `0xRRGGBBAA`.

use gpui::WindowBackgroundAppearance;
use vials_core::model::VialTheme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// Blurred = Windows acrylic behind the panel; Transparent = no blur (Classic).
    pub material: WindowBackgroundAppearance,
    pub panel: u32,
    pub border: u32,
    pub text: u32,
    pub line: u32,
    pub hover: u32,
    pub hover_border: u32,
    pub selected: u32,
    pub selected_border: u32,
    pub placeholder: u32,
    pub radius: f32,
    /// The glass tint setting as 0.0 (clear) to 1.0 (dark): used by the Liquid backdrop.
    pub tint: f32,
    /// Liquid glass draws its own backdrop (see `liquid.rs`) instead of the window material.
    pub liquid: bool,
}

impl Theme {
    /// The look with the panel in `rgb` (`0xRRGGBB`), keeping its see-through amount. Liquid has no panel to colour.
    pub fn with_color(mut self, rgb: u32) -> Self {
        if !self.liquid {
            self.panel = (rgb << 8) | (self.panel & 0xff);
        }
        self
    }

    /// `tint` is the "glass tint" setting, 0 (clearest) to 100 (nearly solid).
    pub fn of(theme: VialTheme, tint: u8) -> Self {
        let tint = u32::from(tint.min(100));
        // The panel's own alpha follows the tint: 0 -> 8 %, 100 -> 85 %.
        let glass_alpha = 0x14 + (0xd8 - 0x14) * tint / 100;
        let glass = Theme {
            material: WindowBackgroundAppearance::Blurred,
            panel: 0x181c2600 | glass_alpha,
            border: 0xffffff42,
            text: 0xffffffff,
            line: 0xffffff24,
            hover: 0xffffff28,
            hover_border: 0xffffff57,
            selected: 0x78afff48,
            selected_border: 0xaad2ffb4,
            placeholder: 0xffffff18,
            radius: 12.0,
            tint: tint as f32 / 100.0,
            liquid: false,
        };
        match theme {
            VialTheme::System => {
                // Dark unless Windows says its apps are light.
                let light = vials_platform::apps_use_light_theme().unwrap_or(false);
                Theme::of(if light { VialTheme::FluentLight } else { VialTheme::Fluent }, tint as u8)
            }
            VialTheme::Glass => glass,
            VialTheme::Liquid => Theme { liquid: true, material: WindowBackgroundAppearance::Transparent, panel: 0x00000000, border: 0x00000000, ..glass },
            VialTheme::Classic => Theme {
                material: WindowBackgroundAppearance::Transparent,
                panel: 0x181b20e6,
                border: 0xffffff30,
                radius: 8.0,
                ..glass
            },
            VialTheme::Fluent => Theme {
                panel: 0x1b252cf2,
                border: 0xffffff1c,
                hover: 0xffffff14,
                hover_border: 0xffffff2a,
                radius: 8.0,
                ..glass
            },
            VialTheme::FluentLight => Theme {
                panel: 0xf4f7f8f2,
                border: 0x00000020,
                text: 0x1b1b1bff,
                line: 0x00000018,
                hover: 0x00000012,
                hover_border: 0x00000024,
                selected: 0x0067c030,
                selected_border: 0x0067c0a0,
                placeholder: 0x00000010,
                radius: 8.0,
                ..glass
            },
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_panel_colour_keeps_the_see_through_amount_and_liquid_has_none() {
        let glass = Theme::of(VialTheme::Glass, 40);
        let blue = glass.with_color(0x1f3b73);
        assert_eq!(blue.panel >> 8, 0x1f3b73);
        assert_eq!(blue.panel & 0xff, glass.panel & 0xff);
        let liquid = Theme::of(VialTheme::Liquid, 40);
        assert_eq!(liquid.with_color(0x1f3b73).panel, liquid.panel);
    }

    use super::*;

    #[test]
    fn glass_gets_more_solid_with_the_tint() {
        let alpha = |t| Theme::of(VialTheme::Glass, t).panel & 0xff;
        assert!(alpha(0) < alpha(20) && alpha(20) < alpha(100));
        assert_eq!(alpha(200), alpha(100), "the tint is capped at 100");
    }

    #[test]
    fn only_classic_has_no_blur_and_only_liquid_draws_its_own_backdrop() {
        for t in [VialTheme::Glass, VialTheme::Fluent, VialTheme::FluentLight, VialTheme::System] {
            assert_eq!(Theme::of(t, 20).material, WindowBackgroundAppearance::Blurred);
        }
        for t in [VialTheme::Classic, VialTheme::Liquid] {
            assert_eq!(Theme::of(t, 20).material, WindowBackgroundAppearance::Transparent, "no Windows blur behind {t:?}");
        }
        assert!(Theme::of(VialTheme::Liquid, 20).liquid && !Theme::of(VialTheme::Glass, 20).liquid);
    }

    #[test]
    fn the_light_theme_has_dark_text() {
        assert_ne!(Theme::of(VialTheme::FluentLight, 20).text, Theme::of(VialTheme::Glass, 20).text);
    }
}
