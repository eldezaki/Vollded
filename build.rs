// Build script: embeds assets/icon.ico into the .exe so Explorer shows the app icon.
// Only runs for Windows targets; if the resource compiler is missing the build still succeeds
// (the window/taskbar icon comes from assets/icon.png at runtime either way).
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").map_or(false, |t| t == "windows");
    if windows && std::path::Path::new("assets/icon.ico").exists() {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the exe icon: {e}");
        }
    }
}
