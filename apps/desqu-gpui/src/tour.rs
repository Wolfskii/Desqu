//! The short walk through Desqu shown the first time it opens: what it is, then one step for each app.

use crate::modules::Kind;

/// One step of the tour.
pub struct Step {
    pub title: &'static str,
    pub text: &'static str,
    /// The app the step is about (it can be turned on from the step), if any.
    pub app: Option<Kind>,
}

pub const STEPS: [Step; 4] = [
    Step {
        title: "Welcome to Desqu",
        text: "Desqu keeps a few small tools together: it starts them, stops them and holds their settings. Turn on only what you want; each tool runs on its own.",
        app: None,
    },
    Step {
        title: "Vials",
        text: "Boxes on your desktop for icons. Drag a box on the empty desktop to make one, drop icons in, and the desktop stays tidy. A vial can also show a folder, your recent files or notes.",
        app: Some(Kind::Vials),
    },
    Step {
        title: "Workspaces",
        text: "Gives each virtual desktop its own desktop icons. Switch desktops with Win+Tab and the icons change with them.",
        app: Some(Kind::Workspaces),
    },
    Step {
        title: "Screenbound",
        text: "Keeps fullscreen apps inside a part of your screen that you choose, so everything else stays in view. Pick the zone and the apps on its page.",
        app: Some(Kind::Screenbound),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::ALL;

    #[test]
    fn the_tour_starts_with_a_welcome_and_has_a_step_for_every_app() {
        assert!(STEPS[0].app.is_none());
        for kind in ALL {
            assert!(STEPS.iter().any(|s| s.app == Some(kind)), "{}", kind.name());
        }
        assert!(STEPS.iter().all(|s| !s.title.is_empty() && s.text.len() > 40));
    }

    #[test]
    fn the_tour_and_the_descriptions_of_the_apps_are_translated() {
        use desqu_native_shell::i18n::{is_translated, Lang};
        for lang in [Lang::Sv, Lang::Pl] {
            for step in &STEPS {
                assert!(is_translated(lang, step.title) && is_translated(lang, step.text), "{} in {}", step.title, lang.code());
            }
            for kind in ALL {
                assert!(is_translated(lang, kind.description()), "{} in {}", kind.name(), lang.code());
            }
        }
    }
}
