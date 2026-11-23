//! Embeds the icon and the version information into the executable (Windows only).

fn main() {
    println!("cargo:rerun-if-changed=assets/screenbound.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/screenbound.ico");
        res.set("ProductName", "Screenbound");
        res.set("FileDescription", "Screenbound: keep fullscreen apps inside a zone of your monitor");
        res.set("CompanyName", "Desqu contributors");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
