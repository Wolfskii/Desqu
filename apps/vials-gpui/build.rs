//! Embeds the icon and the version information into the executable (Windows only).

fn main() {
    println!("cargo:rerun-if-changed=assets/vials.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/vials.ico");
        res.set("ProductName", "Vials");
        res.set("FileDescription", "Vials: desktop containers for shortcuts, apps and folders");
        res.set("CompanyName", "Desqu contributors");
        if let Err(e) = res.compile() {
            // Not fatal: the app works without its icon.
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
