//! Embeds the icon and the version information into the executable (Windows only).

fn main() {
    println!("cargo:rerun-if-changed=assets/desqu.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/desqu.ico");
        res.set("ProductName", "Desqu");
        res.set("FileDescription", "Desqu: small desktop utilities for Windows");
        res.set("CompanyName", "Desqu contributors");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
