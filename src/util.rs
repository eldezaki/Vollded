//! Small helpers for reading the Vollded folder: listing files, sizes, scanning the
//! UGC folders, loading saved metadata, and validating the published link.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use crate::model::{Item, Meta};

/// All FILES (not folders) directly inside `dir`, sorted by name.
pub fn list_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// All files inside `dir` and its sub-folders, up to `depth` levels (recursion!).
pub fn list_files_recursive(dir: &Path, depth: u8) -> Vec<PathBuf> {
    let mut out = vec![];
    if depth == 0 {
        return out;
    }
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(list_files_recursive(&p, depth - 1)); // call ourselves on the sub-folder
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Total size in bytes of everything inside a folder.
pub fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if let Ok(m) = e.metadata() {
                total += if m.is_dir() { dir_size(&e.path()) } else { m.len() };
            }
        }
    }
    total
}

/// 1_500_000 → "1.5 MB"
pub fn fmt_bytes(n: u64) -> String {
    let n = n as f64;
    if n >= 1e9 {
        format!("{:.2} GB", n / 1e9)
    } else if n >= 1e6 {
        format!("{:.1} MB", n / 1e6)
    } else {
        format!("{:.0} KB", n / 1e3)
    }
}

/// File name of a path as a String ("C:\a\b.txt" → "b.txt").
pub fn fname(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

/// Windows paths are case-insensitive, so compare them that way.
pub fn same_path(a: &Path, b: &Path) -> bool {
    a == b || a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy())
}

/// Scans the Vollded folder: every sub-folder becomes an `Item`.
pub fn scan(root: &Path) -> Vec<Item> {
    let mut items = vec![];
    if let Ok(rd) = fs::read_dir(root) {
        for e in rd.flatten() {
            let p = e.path();
            let id = e.file_name().to_string_lossy().to_string();
            if p.is_dir() && !id.starts_with('.') {
                items.push(Item::load(id, p));
            }
        }
    }
    // Sort numerically ("2" before "10"); non-numeric names go last.
    items.sort_by(|a, b| match (a.id.parse::<u64>(), b.id.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        (Ok(_), Err(_)) => std::cmp::Ordering::Less,
        (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
        _ => a.id.cmp(&b.id),
    });
    items
}

/// Statuses + notes live in a hidden JSON file inside the Vollded folder.
pub fn meta_path(root: &Path) -> PathBuf {
    root.join(".ugc_manager.json")
}

pub fn load_meta(root: &Path) -> HashMap<String, Meta> {
    fs::read_to_string(meta_path(root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// A published-UGC link must be a full http(s) URL with a real-looking host and no spaces.
pub fn valid_link(s: &str) -> bool {
    let s = s.trim();
    if !(s.starts_with("https://") || s.starts_with("http://")) || s.contains(char::is_whitespace) {
        return false;
    }
    // host = text between "://" and the next "/"
    let rest = s.split("://").nth(1).unwrap_or("");
    let host = rest.split('/').next().unwrap_or("");
    host.contains('.') && host.len() > 3
}
