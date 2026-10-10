const COMMANDS: &[&str] = &[
    "get_config",
    "set_config",
    "list_desktops",
    "list_items",
    "item_icon",
    "capture_workspace",
    "restore_workspace",
    "desktop_status",
    "module_start",
    "module_stop",
    "module_status",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
