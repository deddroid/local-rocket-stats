fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    // Icon and version info embedded in the Windows executable.
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Local Rocket Stats");
        res.set(
            "FileDescription",
            "Local Rocket Stats - Rocket League stats and MMR tracker",
        );
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the Windows icon: {e}");
        }
    }
}
