const COMMANDS: &[&str] = &[
    "get_config",
    "set_config",
    "list_desktops",
    "list_items",
    "item_icon",
    "assign_item",
    "desktop_status",
    "module_start",
    "module_stop",
    "module_status",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
