const COMMANDS: &[&str] = &[
    "get_monitors",
    "get_config",
    "set_config",
    "set_enabled",
    "get_status",
    "list_windows",
    "get_activity",
    "release_window",
    "release_all",
    "module_start",
    "module_stop",
    "module_status",
    "legacy_status",
    "legacy_uninstall_old",
    "legacy_delete_settings_backup",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
