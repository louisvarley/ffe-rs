//! On Windows, give `ffe-launcher.exe` its icon and version details. A
//! missing resource compiler only loses the icon; it never fails the build.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/icon.ico")
            .set(
                "FileDescription",
                "Frontier: First Encounters (Elite 3) Launcher",
            )
            .set("ProductName", "ffe-rs");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=launcher icon not embedded: {error}");
        }
    }
}
