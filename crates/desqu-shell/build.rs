const COMMANDS: &[&str] = &["get_shell_settings", "set_shell_settings", "app_info"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).build();
}
