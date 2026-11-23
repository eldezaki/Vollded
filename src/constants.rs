//! Fixed values: the name / description limits, the GitHub link, and the list of
//! components shown in the About pop-up.

/// `const` = a value fixed at compile time. Names are SHOUTING_CASE by convention.
pub const MAX_NAME: usize = 50; // max characters allowed for a UGC name
pub const MAX_DESC: usize = 1000; // max characters allowed in a description
pub const GITHUB_URL: &str = "https://github.com/eldezaki/Vollded";

/// Everything the app is built with, shown in the About pop-up: (name, what it is used for).
pub const COMPONENTS: [(&str, &str); 9] = [
    ("Rust (2021 edition)", "Programming language and standard library"),
    ("eframe / egui 0.29", "Window, rendering and the immediate-mode UI"),
    ("winit + glow (OpenGL)", "Window events, drag & drop and graphics (via eframe)"),
    ("image 0.25", "Decodes JPEG/PNG for the reference preview and app icon"),
    ("rfd 0.15", "Native folder picker dialog"),
    ("open 5", "Opens files, folders and links with the default app"),
    ("trash 5", "Moves replaced and deleted files to the Recycle Bin"),
    ("serde + serde_json", "Saves settings, statuses and notes as JSON"),
    ("winresource 0.1", "Embeds the icon into the .exe at build time"),
];
