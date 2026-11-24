//! The Windows plumbing every Desqu GPUI app shares: [`tray`] (one running copy, a tray icon with a
//! menu, actions from second launches), [`autostart`] (start with Windows) and [`update`] (is there a
//! newer version). Each app says who it is with an [`Identity`].

pub mod autostart;
pub mod crash;
pub mod hotkey;
pub mod i18n;
pub mod power;
pub mod stdio;
pub mod tray;
pub mod update;
pub mod windowtools;

/// Who the app is: names derived from it keep two apps from ever sharing a mutex, a window class or a
/// startup entry.
#[derive(Debug, Clone, Copy)]
pub struct Identity {
    /// What the user sees: the tray tooltip.
    pub name: &'static str,
    /// Letters and digits only: the base of the mutex, window class and startup entry names.
    pub key: &'static str,
}

impl Identity {
    pub fn mutex_name(&self) -> String {
        format!("Local\\{}-single-instance", self.key)
    }
    pub fn window_class(&self) -> String {
        format!("{}TrayWindow", self.key)
    }
    /// The name of the app's value in the Run key.
    pub fn startup_name(&self) -> &'static str {
        self.key
    }
    /// Tags the data a second launch sends, so another app's window never takes it for its own.
    pub fn data_tag(&self) -> usize {
        self.key.bytes().fold(0x5649_414Cusize, |h, b| h.wrapping_mul(31).wrapping_add(usize::from(b)))
    }
}

#[cfg(test)]
mod tests {
    use super::Identity;

    #[test]
    fn two_apps_never_share_a_name() {
        let (a, b) = (Identity { name: "Vials", key: "VialsGpui" }, Identity { name: "Workspaces", key: "WorkspacesGpui" });
        assert_ne!(a.mutex_name(), b.mutex_name());
        assert_ne!(a.window_class(), b.window_class());
        assert_ne!(a.startup_name(), b.startup_name());
        assert_ne!(a.data_tag(), b.data_tag());
        assert_eq!(a.mutex_name(), "Local\\VialsGpui-single-instance");
    }
}
