//! Portable settings: remembered in `vollded_ugc.json` next to the exe
//! (nothing is written to AppData or the registry).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Saved next to the .exe so the app is portable (no AppData / registry).
/// `#[derive(...)]` asks the compiler to auto-generate trait implementations for us.
#[derive(Default, Serialize, Deserialize)]
pub struct Config {
    pub root: String,
}

/// Directory that contains our .exe (falls back to "." if unknown).
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok() // Result → Option (drops the error)
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn cfg_path() -> PathBuf {
    exe_dir().join("vollded_ugc.json")
}

/// Reads the remembered Vollded folder from the config file next to the exe.
/// The `?` operator means: "if this is None/Err, return early with None/Err".
pub fn load_cfg_root() -> Option<PathBuf> {
    let cfg: Config = serde_json::from_str(&fs::read_to_string(cfg_path()).ok()?).ok()?;
    if cfg.root.is_empty() {
        return None;
    }
    let p = PathBuf::from(&cfg.root);
    // A relative path is relative to the exe, which keeps the whole folder movable.
    Some(if p.is_relative() { exe_dir().join(p) } else { p })
}

pub fn save_cfg_root(root: &Path) {
    // Store the path relative to the exe when possible.
    let stored = root
        .strip_prefix(exe_dir())
        .map(|r| r.to_path_buf())
        .unwrap_or_else(|_| root.to_path_buf());
    let cfg = Config { root: stored.display().to_string() };
    if let Ok(s) = serde_json::to_string_pretty(&cfg) {
        let _ = fs::write(cfg_path(), s); // `let _ =` = deliberately ignore a possible error
    }
}
