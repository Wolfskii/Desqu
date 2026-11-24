//! Embeds the icon and the version information into the executable (Windows only).

fn main() {
    println!("cargo:rerun-if-changed=assets/workspaces.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/workspaces.ico");
        res.set("ProductName", "Workspaces");
        res.set("FileDescription", "Workspaces: desktop icons that belong to a virtual desktop");
        res.set("CompanyName", "Desqu contributors");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
