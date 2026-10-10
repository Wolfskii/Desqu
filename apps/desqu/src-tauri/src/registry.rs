//! The modules Desqu can host. Each one is a library crate exposing a Tauri plugin and the shell's
//! `Module` trait; Desqu runs them in its own process.

use std::path::Path;
use std::sync::Arc;

use desqu_shell::module::Module;
use tauri::plugin::TauriPlugin;
use tauri::Wry;

/// How the same utility looks when installed as its own app (decision D2: Desqu takes over from it).
#[derive(Debug, Clone, Copy)]
pub struct Standalone {
    /// Executable name of the standalone app, used to find and ask it to quit.
    pub exe_name: &'static str,
    /// Bundle identifier: its settings live in `%APPDATA%\<identifier>`, shared with Desqu.
    pub identifier: &'static str,
    /// `HKCU\...\Uninstall\<key>` of its installer.
    pub uninstall_key: &'static str,
    /// Value name of its "start with system" Run entry.
    pub run_value: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct Descriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub version: &'static str,
    /// `false` on operating systems without a backend for this module.
    pub supported: bool,
    /// Files in the settings folder that "Reset settings" deletes.
    pub settings_files: &'static [&'static str],
    pub standalone: Standalone,
}

/// Builds a module's Tauri plugin; the argument is the per-user Roaming AppData folder.
pub type PluginInit = fn(&Path) -> TauriPlugin<Wry>;

#[derive(Clone)]
pub struct Entry {
    pub descriptor: Descriptor,
    pub module: Arc<dyn Module<Wry>>,
    pub init: PluginInit,
}

fn init_screenbound(roaming: &Path) -> TauriPlugin<Wry> {
    // Same folder as the standalone app, so both read and write the same settings (D2).
    let dir = roaming.join("io.github.wolfskii.screenbound");
    screenbound_module::init(screenbound_module::ModuleOptions {
        config_dir: dir.clone(),
        data_dir: dir,
        legacy_dir: Some(screenbound_module::migrate::legacy_dir(roaming)),
        // Desqu starts modules itself, according to its own settings.
        start_immediately: false,
    })
}

/// Static facts about every hosted module. Kept free of the window runtime so unit tests can use it
/// (a test binary that links Wry cannot start on Windows without the app manifest).
pub fn descriptors() -> Vec<Descriptor> {
    vec![Descriptor {
        id: "screenbound",
        name: "Screenbound",
        description: "Keep fullscreen windows inside a zone of your monitor instead of covering all of it.",
        version: screenbound_module::VERSION,
        supported: cfg!(windows),
        settings_files: &["config.json"],
        standalone: Standalone {
            exe_name: "screenbound.exe",
            identifier: "io.github.wolfskii.screenbound",
            uninstall_key: "Screenbound",
            run_value: "Screenbound",
        },
    }]
}

/// Descriptors joined with the running module and its plugin builder.
pub fn entries() -> Vec<Entry> {
    descriptors()
        .into_iter()
        .filter_map(|descriptor| {
            let (module, init): (Arc<dyn Module<Wry>>, PluginInit) = match descriptor.id {
                "screenbound" => (Arc::new(screenbound_module::ScreenboundModule), init_screenbound),
                _ => return None,
            };
            Some(Entry { descriptor, module, init })
        })
        .collect()
}

pub fn find(id: &str) -> Option<Entry> {
    entries().into_iter().find(|e| e.descriptor.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let all = descriptors();
        let mut ids: Vec<_> = all.iter().map(|d| d.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
    }

    #[test]
    fn descriptor_ids_match_the_module_crates() {
        let sb = descriptors().into_iter().find(|d| d.id == "screenbound").unwrap();
        assert_eq!(sb.id, screenbound_module::plugin::MODULE_ID);
    }

    #[test]
    fn every_module_declares_what_reset_deletes_and_its_standalone_app() {
        for d in descriptors() {
            assert!(!d.settings_files.is_empty(), "{}", d.id);
            assert!(d.standalone.exe_name.ends_with(".exe"), "{}", d.id);
            assert!(!d.standalone.identifier.is_empty(), "{}", d.id);
        }
    }
}
