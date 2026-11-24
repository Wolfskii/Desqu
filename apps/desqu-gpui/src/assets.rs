//! The icons of the window, built into the program. They are from Lucide (ISC license, see
//! `assets/icons/LICENSE-lucide.txt`) and drawn in the colour of the text around them.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

const ICONS: [(&str, &[u8]); 7] = [
    ("icons/house.svg", include_bytes!("../assets/icons/house.svg")),
    ("icons/settings.svg", include_bytes!("../assets/icons/settings.svg")),
    ("icons/scan.svg", include_bytes!("../assets/icons/scan.svg")),
    ("icons/flask-round.svg", include_bytes!("../assets/icons/flask-round.svg")),
    ("icons/layers.svg", include_bytes!("../assets/icons/layers.svg")),
    ("icons/chevron-right.svg", include_bytes!("../assets/icons/chevron-right.svg")),
    ("icons/wrench.svg", include_bytes!("../assets/icons/wrench.svg")),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS.iter().find(|(name, _)| *name == path).map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS.iter().filter(|(name, _)| name.starts_with(path)).map(|(name, _)| SharedString::from(*name)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_there_and_is_an_svg() {
        for (name, _) in ICONS {
            let bytes = Assets.load(name).unwrap().unwrap_or_else(|| panic!("{name} is missing"));
            assert!(String::from_utf8_lossy(&bytes).contains("<svg"), "{name} is not an svg");
        }
        assert!(Assets.load("icons/nope.svg").unwrap().is_none());
    }

    #[test]
    fn every_app_has_an_icon() {
        for kind in crate::modules::ALL {
            assert!(Assets.load(kind.icon()).unwrap().is_some(), "{}", kind.name());
        }
    }
}
