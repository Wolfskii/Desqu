const COMMANDS: &[&str] = &[
    "get_config",
    "set_config",
    "list_items",
    "item_icon",
    "assign_item",
    "open_item",
    "create_vial_from_menu",
    "dismiss_menu",
    "menu_ready",
    "begin_rename",
    "end_rename",
    "drop_items",
    "drag_item_out",
    "show_item_menu",
    "get_monitors",
    "desktop_status",
    "module_start",
    "module_stop",
    "module_status",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
