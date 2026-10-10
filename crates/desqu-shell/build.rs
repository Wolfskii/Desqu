const COMMANDS: &[&str] = &[
    "get_shell_settings",
    "set_shell_settings",
    "app_info",
    "update_status",
    "check_update",
    "install_update",
    "open_releases_page",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
