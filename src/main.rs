// ════════════════════════════════════════════════════════════════════════════
//  Vollded UGC — a portable manager for Roblox UGC project folders.
//
//  Reading guide (for learning Rust):
//    1. Imports & constants          5. The `App` struct and its methods
//    2. Theme / styling              6. Drag & drop logic
//    3. Plain data types (structs)   7. Small UI helper functions
//    4. File-system helper functions 8. `update()` (runs every frame) and `main()`
//
//  egui is an "immediate mode" GUI: `update()` is called whenever something
//  happens (mouse move, click, key…) and we describe the WHOLE interface again
//  from scratch each time. There is no widget tree to keep in sync.
// ════════════════════════════════════════════════════════════════════════════

// On Windows release builds, don't pop up a black console window behind the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// ───────────────────────── 1. Imports & constants ─────────────────────────
// `use` brings names into scope so we can write `Color32` instead of `eframe::egui::Color32`.
use eframe::egui::{
    self, load::SizedTexture, Align, Align2, Color32, CursorIcon, FontId, Layout, Rect, RichText,
    Sense, Stroke, Vec2,
};
use serde::{Deserialize, Serialize}; // lets us turn structs into JSON and back
use std::collections::{HashMap, HashSet}; // HashMap = key → value dictionary, HashSet = unique values
use std::fs; // file system: read, write, copy, delete…
use std::path::{Path, PathBuf}; // `Path` = borrowed path, `PathBuf` = owned path (like &str vs String)
use std::sync::mpsc::{channel, Receiver, Sender}; // a pipe to send data between threads
use std::time::{Duration, Instant};

/// `const` = a value fixed at compile time. Names are SHOUTING_CASE by convention.
const MAX_NAME: usize = 50; // max characters allowed for a UGC name
const MAX_DESC: usize = 1000; // max characters allowed in a description
const GITHUB_URL: &str = "https://github.com/eldezaki/Vollded";

/// Everything the app is built with, shown in the About pop-up: (name, what it is used for).
const COMPONENTS: [(&str, &str); 9] = [
    (
        "Rust (2021 edition)",
        "Programming language and standard library",
    ),
    (
        "eframe / egui 0.29",
        "Window, rendering and the immediate-mode UI",
    ),
    (
        "winit + glow (OpenGL)",
        "Window events, drag & drop and graphics (via eframe)",
    ),
    (
        "image 0.25",
        "Decodes JPEG/PNG for the reference preview and app icon",
    ),
    ("rfd 0.15", "Native folder picker dialog"),
    (
        "open 5",
        "Opens files, folders and links with the default app",
    ),
    (
        "trash 5",
        "Moves replaced and deleted files to the Recycle Bin",
    ),
    (
        "serde + serde_json",
        "Saves settings, statuses and notes as JSON",
    ),
    (
        "winresource 0.1",
        "Embeds the icon into the .exe at build time",
    ),
];

// ───────────────────────── 2. Theme / styling ─────────────────────────
// `Color32::from_rgb(r, g, b)` is a `const fn`, so colors can be constants too.
const BG: Color32 = Color32::from_rgb(15, 15, 18);
const PANEL: Color32 = Color32::from_rgb(21, 21, 26);
const CARD: Color32 = Color32::from_rgb(31, 31, 38);
const CARD_HOVER: Color32 = Color32::from_rgb(42, 42, 52);
const ACCENT: Color32 = Color32::from_rgb(225, 48, 60); // the red theme color
const GOOD: Color32 = Color32::from_rgb(52, 211, 153); // green
const WARN: Color32 = Color32::from_rgb(251, 191, 36); // yellow
const ERR: Color32 = Color32::from_rgb(255, 80, 80); // bright red for invalid input
const BAD: Color32 = Color32::from_rgb(120, 126, 148); // grey used for "missing" marks
const TEXT: Color32 = Color32::from_rgb(232, 234, 242);
const MUTED: Color32 = Color32::from_rgb(138, 145, 168);

/// Applies our dark/red look to egui. Called once at startup.
fn setup_style(ctx: &egui::Context) {
    // `let mut` = a variable we are allowed to change (variables are immutable by default).
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = CARD;
    v.extreme_bg_color = BG; // background of text boxes
    v.override_text_color = Some(TEXT); // `Some(x)` = "there is a value" (Option type)
    v.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, ACCENT);

    // Give every kind of widget rounded corners. We loop over mutable references (`&mut`)
    // so that changing `w` changes the original value inside `v`.
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.rounding = 8.0.into();
    }
    v.widgets.inactive.weak_bg_fill = CARD_HOVER;
    v.widgets.inactive.bg_fill = CARD_HOVER;
    v.widgets.hovered.weak_bg_fill = ACCENT.gamma_multiply(0.45);
    v.widgets.hovered.bg_fill = ACCENT.gamma_multiply(0.45);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(40, 40, 52));
    ctx.set_visuals(v);

    // Spacing and CPU-saving options live in `Style`.
    let mut s = (*ctx.style()).clone(); // `*` reads through the Arc, `.clone()` makes our own copy
    s.spacing.item_spacing = Vec2::new(10.0, 10.0);
    s.spacing.button_padding = Vec2::new(12.0, 6.0);
    s.animation_time = 0.0; // no hover fades → fewer repaints
    s.interaction.selectable_labels = false; // labels aren't text-selectable → less work per frame
    ctx.set_style(s);
}

// ───────────────────────── 3. Plain data types ─────────────────────────

/// Saved next to the .exe so the app is portable (no AppData / registry).
/// `#[derive(...)]` asks the compiler to auto-generate trait implementations for us.
#[derive(Default, Serialize, Deserialize)]
struct Config {
    root: String,
}

/// The progress state of one UGC. An `enum` is "one of these variants".
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
enum Status {
    #[default] // `Status::default()` gives Todo
    Todo,
    Modeling,
    Texturing,
    Ready,
    Uploaded,
}

impl Status {
    /// All variants in display order (a fixed-size array: `[Type; length]`).
    const ALL: [Status; 5] = [
        Status::Todo,
        Status::Modeling,
        Status::Texturing,
        Status::Ready,
        Status::Uploaded,
    ];

    /// Text shown in the UI. `match` must cover every variant (the compiler checks!).
    fn label(self) -> &'static str {
        match self {
            Status::Todo => "To do",
            Status::Modeling => "Modeling",
            Status::Texturing => "Texturing",
            Status::Ready => "Ready",
            Status::Uploaded => "Uploaded",
        }
    }

    /// Color of the little status dot / chip.
    fn color(self) -> Color32 {
        match self {
            Status::Todo => MUTED,
            Status::Modeling => WARN,
            Status::Texturing => Color32::from_rgb(96, 165, 250),
            Status::Ready => GOOD,
            Status::Uploaded => ACCENT,
        }
    }
}

/// Things the user types/chooses that are NOT stored in the UGC folder itself.
#[derive(Clone, Default, Serialize, Deserialize)]
struct Meta {
    status: Status,
    notes: String,
    /// Link to the published UGC. Required to set the status to Uploaded.
    /// `#[serde(default)]` = old save files without this field still load fine.
    #[serde(default)]
    link: String,
}

/// Everything we know about ONE UGC folder (e.g. folder "1"), filled in by `Item::load`.
#[derive(Default)]
struct Item {
    id: String,                   // the folder name, e.g. "1"
    dir: PathBuf,                 // full path of the folder
    raw_blend: Option<PathBuf>,   // "Model"  (`Option` = might be missing)
    ready_blend: Option<PathBuf>, // "Ready"
    reference: Option<PathBuf>,   // reference image
    spp: Option<PathBuf>,         // "Texture" (Substance Painter project)
    txt: Option<PathBuf>,         // description file; its NAME is the UGC name
    previews: Vec<PathBuf>,       // `Vec` = growable list
    textures: Vec<PathBuf>,
    title: String,
    title_lc: String, // lower-case title, cached so searching doesn't re-lowercase every frame
    description: String,
    desc_saved: bool,   // true if the .txt on disk has real text in it
    dirty: bool,        // true if the description was edited but not saved yet
    size_bytes: u64,    // total size of the whole folder
    size_label: String, // the same, as text like "86.2 MB"
    /// File sizes are read once while scanning, never per frame.
    sizes: HashMap<PathBuf, String>,
}

/// How many checklist items count towards the progress bar.
const CHECKS: usize = 5;

impl Item {
    /// Reads a folder from disk and works out which file is which.
    fn load(id: String, dir: PathBuf) -> Item {
        // `..Default::default()` fills all other fields with their default (None, empty, 0…).
        let mut it = Item {
            id: id.clone(),
            dir: dir.clone(),
            ..Default::default()
        };

        // Look at every file directly inside the folder and classify it by extension.
        for f in list_files(&dir) {
            let stem = f
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = f
                .extension()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let lstem = stem.to_lowercase();
            match ext.as_str() {
                "blend" => {
                    if lstem.ends_with("_ready") {
                        it.ready_blend = Some(f); // "1_Ready.blend" → Ready
                    } else if stem == id || it.raw_blend.is_none() {
                        it.raw_blend = Some(f); // "1.blend" → Model
                    }
                }
                "jpg" | "jpeg" | "png" => {
                    if it.reference.is_none() || stem == id {
                        it.reference = Some(f);
                    }
                }
                "spp" => {
                    // ignore "…_autosave_0.spp" backups
                    if !lstem.contains("autosave") && (it.spp.is_none() || stem == id) {
                        it.spp = Some(f);
                    }
                }
                "txt" => it.txt = Some(f),
                _ => {} // anything else (like .blend1) is ignored
            }
        }

        // previews/ folder: only video files
        it.previews = list_files(&dir.join("previews"))
            .into_iter()
            .filter(|p| {
                matches!(
                    p.extension()
                        .map(|e| e.to_string_lossy().to_lowercase())
                        .as_deref(),
                    Some("mp4" | "mov" | "webm" | "mkv")
                )
            })
            .collect();
        // textures/ folder: every file (up to 3 levels deep)
        it.textures = list_files_recursive(&dir.join("textures"), 3);

        // Title = .txt file name, description = .txt contents.
        if let Some(t) = &it.txt {
            it.title = t
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            it.description = fs::read_to_string(t).unwrap_or_default();
            it.desc_saved = !it.description.trim().is_empty();
        } else {
            it.title = format!("UGC #{}", it.id);
        }
        it.title_lc = it.title.to_lowercase();

        // Folder size + the sizes of the files we show in the checklist/previews.
        it.size_bytes = dir_size(&dir);
        it.size_label = fmt_bytes(it.size_bytes);
        let known: Vec<PathBuf> = [&it.raw_blend, &it.ready_blend, &it.reference, &it.spp]
            .into_iter()
            .flatten() // skips the `None`s and unwraps the `Some`s
            .cloned()
            .chain(it.previews.iter().cloned())
            .collect();
        for p in known {
            let n = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            it.sizes.insert(p, fmt_bytes(n));
        }
        it
    }

    /// Looks up a pre-computed file size label (returns "" if unknown).
    fn size(&self, p: &Path) -> &str {
        self.sizes.get(p).map(|s| s.as_str()).unwrap_or("")
    }

    /// Which of the 5 checklist items are done.
    fn checks(&self) -> [bool; CHECKS] {
        [
            self.reference.is_some(),
            self.raw_blend.is_some(),
            self.spp.is_some(),
            self.ready_blend.is_some(),
            self.desc_saved,
        ]
    }

    /// 0.0 … 1.0
    fn progress(&self) -> f32 {
        self.checks().iter().filter(|c| **c).count() as f32 / CHECKS as f32
    }
}

// ───────────────────────── 4. File-system helpers ─────────────────────────

/// Directory that contains our .exe (falls back to "." if unknown).
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok() // Result → Option (drops the error)
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn cfg_path() -> PathBuf {
    exe_dir().join("vollded_ugc.json")
}

/// Reads the remembered Vollded folder from the config file next to the exe.
/// The `?` operator means: "if this is None/Err, return early with None/Err".
fn load_cfg_root() -> Option<PathBuf> {
    let cfg: Config = serde_json::from_str(&fs::read_to_string(cfg_path()).ok()?).ok()?;
    if cfg.root.is_empty() {
        return None;
    }
    let p = PathBuf::from(&cfg.root);
    // A relative path is relative to the exe, which keeps the whole folder movable.
    Some(if p.is_relative() {
        exe_dir().join(p)
    } else {
        p
    })
}

fn save_cfg_root(root: &Path) {
    // Store the path relative to the exe when possible.
    let stored = root
        .strip_prefix(exe_dir())
        .map(|r| r.to_path_buf())
        .unwrap_or_else(|_| root.to_path_buf());
    let cfg = Config {
        root: stored.display().to_string(),
    };
    if let Ok(s) = serde_json::to_string_pretty(&cfg) {
        let _ = fs::write(cfg_path(), s); // `let _ =` = deliberately ignore a possible error
    }
}

/// All FILES (not folders) directly inside `dir`, sorted by name.
fn list_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// All files inside `dir` and its sub-folders, up to `depth` levels (recursion!).
fn list_files_recursive(dir: &Path, depth: u8) -> Vec<PathBuf> {
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
fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if let Ok(m) = e.metadata() {
                total += if m.is_dir() {
                    dir_size(&e.path())
                } else {
                    m.len()
                };
            }
        }
    }
    total
}

/// 1_500_000 → "1.5 MB"
fn fmt_bytes(n: u64) -> String {
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
fn fname(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Windows paths are case-insensitive, so compare them that way.
fn same_path(a: &Path, b: &Path) -> bool {
    a == b
        || a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
}

/// Scans the Vollded folder: every sub-folder becomes an `Item`.
fn scan(root: &Path) -> Vec<Item> {
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
fn meta_path(root: &Path) -> PathBuf {
    root.join(".ugc_manager.json")
}

fn load_meta(root: &Path) -> HashMap<String, Meta> {
    fs::read_to_string(meta_path(root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Renames a UGC = renames its .txt file (or creates one). Returns `Err(message)` if invalid.
fn do_rename(item: &mut Item, new: &str) -> Result<(), String> {
    let new = new.trim();
    if new.is_empty() {
        return Err("Name can't be empty.".into());
    }
    if new.chars().count() > MAX_NAME {
        return Err(format!("Name is too long (max {MAX_NAME} characters)."));
    }
    // Characters Windows doesn't allow in file names.
    if new.chars().any(|c| "\\/:*?\"<>|".contains(c)) || new.ends_with('.') {
        return Err("Name can't contain \\ / : * ? \" < > | or end with a dot.".into());
    }
    let target = item.dir.join(format!("{new}.txt"));
    let same = item.txt.as_ref().map_or(false, |o| same_path(o, &target));
    if target.exists() && !same {
        return Err("A file with that name already exists in this folder.".into());
    }
    match &item.txt {
        Some(old) => fs::rename(old, &target).map_err(|e| e.to_string())?,
        None => fs::write(&target, &item.description).map_err(|e| e.to_string())?,
    }
    item.txt = Some(target);
    item.title = new.to_string();
    item.title_lc = item.title.to_lowercase();
    Ok(())
}

// ───────────────────────── 6a. Drag & drop: planning and copying ─────────────────────────

/// One file operation: copy `src` to `dest`, then delete the `remove` file (if any).
struct Op {
    src: PathBuf,
    dest: PathBuf,
    remove: Option<PathBuf>, // an older file this one replaces under a different name
    label: &'static str,     // "Model", "Texture", …
    existing: Option<String>, // Some(name) if something already exists → needs confirmation
}

/// State of the "Set as Uploaded" pop-up.
struct UploadDialog {
    id: String,   // which UGC
    link: String, // text typed so far
    focus: bool,  // put the cursor in the box when it first opens
}

/// A drop that is waiting for the user to click "Replace" or "Cancel".
struct Pending {
    id: String,
    ops: Vec<Op>,
    rejected: Vec<String>,
}

/// Decides where each dropped file goes. Returns (operations, rejected file names).
///
///   .blend        → Model  (`<id>.blend`, never the Ready file)
///   .spp          → Texture (`<id>.spp`)
///   .jpg / .jpeg  → Reference image (`<id>.jpg`)
///   .txt          → Description (keeps its own name: the name IS the UGC name)
///   .mp4          → previews/
///   .png          → textures/
///   anything else → rejected
fn plan_ops(
    id: &str,
    dir: &Path,
    item: Option<&Item>,
    files: &[PathBuf],
) -> (Vec<Op>, Vec<String>) {
    let mut ops: Vec<Op> = vec![];
    let mut rejected: Vec<String> = vec![];
    // If the user drops two files for the same single slot (two .blend), the last one wins.
    let mut single: HashMap<&'static str, usize> = HashMap::new();

    for src in files {
        let name = fname(src);
        if !src.is_file() {
            rejected.push(name); // folders can't be dropped
            continue;
        }
        let ext = src
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        // A tuple of: (slot key, label, destination path, existing file in that slot).
        let (slot, label, dest, old): (
            Option<&'static str>,
            &'static str,
            PathBuf,
            Option<PathBuf>,
        ) = match ext.as_str() {
            "blend" => (
                Some("model"),
                "Model",
                dir.join(format!("{id}.blend")),
                item.and_then(|i| i.raw_blend.clone()),
            ),
            "spp" => (
                Some("texture"),
                "Texture",
                dir.join(format!("{id}.spp")),
                item.and_then(|i| i.spp.clone()),
            ),
            "jpg" | "jpeg" => (
                Some("reference"),
                "Reference image",
                dir.join(format!("{id}.{ext}")),
                item.and_then(|i| i.reference.clone()),
            ),
            "txt" => (
                Some("description"),
                "Description",
                dir.join(&name),
                item.and_then(|i| i.txt.clone()),
            ),
            "mp4" => (None, "Preview", dir.join("previews").join(&name), None),
            "png" => (None, "Texture map", dir.join("textures").join(&name), None),
            _ => {
                rejected.push(name);
                continue; // skip to the next file
            }
        };

        if same_path(src, &dest) {
            continue; // dropped a file onto itself: nothing to do
        }
        // Name of whatever will be replaced (the old slot file, or a file with the same name).
        let existing = old
            .as_ref()
            .map(|o| fname(o))
            .or_else(|| dest.exists().then(|| fname(&dest)));
        // Only delete the old file if it's a different file than the destination.
        let remove = old.filter(|o| !same_path(o, &dest));
        let op = Op {
            src: src.clone(),
            dest,
            remove,
            label,
            existing,
        };

        if let Some(key) = slot {
            if let Some(&idx) = single.get(key) {
                ops[idx] = op; // same slot dropped twice: keep the later one
                continue;
            }
            single.insert(key, ops.len());
        } else if let Some(pos) = ops.iter().position(|o| o.dest == op.dest) {
            ops[pos] = op; // same destination twice
            continue;
        }
        ops.push(op);
    }
    (ops, rejected)
}

/// Sends a file or folder to the Recycle Bin. NOTHING in this app is ever deleted permanently:
/// every "delete" or "replace" goes through this function.
fn to_recycle_bin(p: &Path) -> Result<(), String> {
    trash::delete(p).map_err(|e| format!("Could not move {} to the Recycle Bin: {e}", fname(p)))
}

/// Copies the files. We COPY (not move) so the originals stay where they are.
/// A file that gets replaced is moved to the Recycle Bin first (a plain copy would overwrite
/// it for good).
fn apply_ops(ops: &[Op]) -> Result<(), String> {
    for op in ops {
        if let Some(parent) = op.dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?; // make previews/ etc. if missing
        }
        // Check that the new file can be read BEFORE touching anything that exists.
        fs::File::open(&op.src).map_err(|e| format!("Can't read {}: {e}", fname(&op.src)))?;
        // Something with the same name is in the way → into the Recycle Bin.
        if op.dest.exists() {
            to_recycle_bin(&op.dest)?; // `?` = stop and return the error if this failed
        }
        fs::copy(&op.src, &op.dest).map_err(|e| {
            format!(
                "Copy failed for {}: {e}. A replaced file can be restored from the Recycle Bin.",
                fname(&op.src)
            )
        })?;
        // An older file stored under a different name is replaced too → Recycle Bin.
        if let Some(old) = &op.remove {
            to_recycle_bin(old)?;
        }
    }
    Ok(())
}

/// " Rejected (unsupported): a.zip, b.exe." or "" if nothing was rejected.
fn rejected_note(r: &[String]) -> String {
    if r.is_empty() {
        String::new()
    } else {
        format!(" Rejected (unsupported): {}.", r.join(", "))
    }
}

/// Renames the folders in `plan` = list of (old id, new id), e.g. [("2","1"), ("1","2")].
///
/// Two phases so ids can swap safely (renaming "1"→"2" directly would collide):
///   1. every affected folder is first moved to a temporary name
///   2. files inside get the new id in their name, then the folder gets its final name
/// Returns Err if phase 1 failed (nothing was changed), or Ok(list of leftover problems).
fn renumber_folders(root: &Path, plan: &[(String, String)]) -> Result<Vec<String>, String> {
    let tmp_of = |n: usize| root.join(format!("__vollded_tmp_{n}"));

    // Phase 1: old name → temporary name. If one fails, undo the ones that worked.
    let mut done: Vec<(PathBuf, PathBuf)> = vec![]; // (temporary, original)
    for (n, (old, _)) in plan.iter().enumerate() {
        let from = root.join(old);
        let tmp = tmp_of(n);
        if let Err(e) = fs::rename(&from, &tmp) {
            for (t, o) in done.iter().rev() {
                let _ = fs::rename(t, o); // roll back
            }
            return Err(format!(
                "Could not rename folder {old}: {e}. Close anything that has files from it open (Blender, Substance, Explorer). Nothing was changed."
            ));
        }
        done.push((tmp, from));
    }

    // Phase 2: rename the files inside, then give the folder its final name.
    let mut problems: Vec<String> = vec![];
    for (n, (old, new)) in plan.iter().enumerate() {
        let tmp = tmp_of(n);
        rename_inner_files(&tmp, old, new, &mut problems);
        if let Err(e) = fs::rename(&tmp, root.join(new)) {
            problems.push(format!("folder {new}: {e}"));
        }
    }
    Ok(problems)
}

/// Inside one UGC folder: "2.blend" → "1.blend", "2_Ready.blend" → "1_Ready.blend",
/// "2.spp" → "1.spp", "2.jpeg" → "1.jpeg", "2_autosave_0.spp" → "1_autosave_0.spp"…
/// The .txt is skipped: its name is the UGC name, not the number.
fn rename_inner_files(dir: &Path, old: &str, new: &str, problems: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let name = fname(&p);
        if name.to_lowercase().ends_with(".txt") {
            continue;
        }
        // Only names that START with the id followed by "." or "_" (so "10.blend" isn't touched when old = "1").
        if let Some(rest) = name.strip_prefix(old) {
            if rest.starts_with('.') || rest.starts_with('_') {
                let target = dir.join(format!("{new}{rest}"));
                if target.exists() {
                    problems.push(format!("{name}: {} already exists", fname(&target)));
                } else if let Err(err) = fs::rename(&p, &target) {
                    problems.push(format!("{name}: {err}"));
                }
            }
        }
    }
}

/// A published-UGC link must be a full http(s) URL with a real-looking host and no spaces.
fn valid_link(s: &str) -> bool {
    let s = s.trim();
    if !(s.starts_with("https://") || s.starts_with("http://")) || s.contains(char::is_whitespace) {
        return false;
    }
    // host = text between "://" and the next "/"
    let rest = s.split("://").nth(1).unwrap_or("");
    let host = rest.split('/').next().unwrap_or("");
    host.contains('.') && host.len() > 3
}

/// Loads the window icon from assets/icon.png. `include_bytes!` bakes the file INTO the exe at
/// compile time, so the exe stays a single portable file.
fn load_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .ok()?
        .to_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

/// Windows only: ask the OS where the mouse cursor is. We declare the one function we need
/// ourselves, so no extra crate is required. (While dragging files from Explorer, Windows
/// doesn't send mouse-move events to the app, so egui's own pointer position would be stale.)
#[cfg(windows)]
mod winapi_min {
    #[repr(C)] // lay the struct out in memory exactly like C does
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }
    #[link(name = "user32")]
    extern "system" {
        pub fn GetCursorPos(p: *mut Point) -> i32;
    }
    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut p = Point { x: 0, y: 0 };
        // Calling a foreign C function is `unsafe`: the compiler can't verify it for us.
        let ok = unsafe { GetCursorPos(&mut p) };
        if ok != 0 {
            Some((p.x, p.y))
        } else {
            None
        }
    }
}

/// Where was the cursor (in egui coordinates, relative to our window) when files were dropped?
fn drop_pos(ctx: &egui::Context) -> Option<egui::Pos2> {
    #[cfg(windows)] // this block only exists in Windows builds
    {
        if let (Some((cx, cy)), Some(inner)) = (
            winapi_min::cursor_pos(),
            ctx.input(|i| i.viewport().inner_rect),
        ) {
            let ppp = ctx.pixels_per_point(); // screen pixels per egui point (DPI scaling)
            return Some(egui::pos2(
                cx as f32 / ppp - inner.min.x,
                cy as f32 / ppp - inner.min.y,
            ));
        }
    }
    ctx.input(|i| i.pointer.hover_pos().or(i.pointer.latest_pos()))
}

// ───────────────────────── Thumbnail (decoded on another thread) ─────────────────────────

/// The reference image, shrunk down so it doesn't waste memory.
struct Thumb {
    path: PathBuf,
    tex: Option<egui::TextureHandle>, // the GPU texture, once decoded
    done: bool,                       // decoding finished (successfully or not)
}
/// Message sent from the decoder thread back to the UI thread.
type ThumbMsg = (PathBuf, Option<egui::ColorImage>);

// ───────────────────────── 5. The App ─────────────────────────

/// All the state of the running program lives in this one struct.
struct App {
    root_input: String, // text in the path box
    root: Option<PathBuf>,
    items: Vec<Item>,
    meta: HashMap<String, Meta>, // folder id → status/notes
    selected: Option<usize>,     // index into `items`
    search: String,
    filter: Option<Status>,     // None = show all statuses
    msg: String,                // message in the bottom bar
    rename_buf: Option<String>, // Some(text) while the rename box is open
    rename_focus: bool,
    // Cached results: recalculated only when something changes (saves CPU).
    visible: Vec<usize>, // indices of items that pass search + filter
    view_dirty: bool,
    last_search: String,
    last_filter: Option<Status>,
    stats: (usize, usize, usize),  // (total, complete, uploaded)
    total_bytes: u64,              // size of the whole Vollded folder
    meta_save_at: Option<Instant>, // when notes were last edited (for delayed saving)
    thumb: Option<Thumb>,
    thumb_tx: Sender<ThumbMsg>,
    thumb_rx: Receiver<ThumbMsg>,
    // Drag & drop
    list_rect: Rect,                     // where the left list was drawn last frame
    main_rect: Rect,                     // where the UGC view was drawn last frame
    pending: Option<Pending>,            // drop waiting for confirmation
    pending_delete: Option<String>, // id of the UGC waiting for "move to Recycle Bin?" confirmation
    upload_dialog: Option<UploadDialog>, // the "paste the published link" pop-up
    show_about: bool,               // is the About pop-up open?
    drag_from: Option<usize>,       // index of the card being dragged to reorder
    last_frame: Instant,            // for the frame-rate limiter
    side_min_w: f32, // narrowest the left panel may get (measured on the first frame)
}

// `impl App { … }` = the methods (functions) that belong to App.
impl App {
    /// Runs once when the program starts.
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        let (tx, rx) = channel(); // a one-way pipe: tx sends, rx receives
        let mut app = App {
            root_input: String::new(),
            root: None,
            items: vec![],
            meta: HashMap::new(),
            selected: None,
            search: String::new(),
            filter: None,
            msg: "Pick your Vollded folder, or drop files onto the list to start.".into(),
            rename_buf: None,
            rename_focus: false,
            visible: vec![],
            view_dirty: true,
            last_search: String::new(),
            last_filter: None,
            stats: (0, 0, 0),
            total_bytes: 0,
            meta_save_at: None,
            thumb: None,
            thumb_tx: tx,
            thumb_rx: rx,
            list_rect: Rect::NOTHING,
            main_rect: Rect::NOTHING,
            pending: None,
            pending_delete: None,
            upload_dialog: None,
            show_about: false,
            drag_from: None,
            last_frame: Instant::now(),
            side_min_w: 0.0,
        };
        // Restore the last folder, or auto-detect a "Vollded" folder next to the exe.
        if let Some(r) = load_cfg_root() {
            app.load_root(r);
        } else {
            let d = exe_dir().join("Vollded");
            if d.is_dir() {
                app.open_root(d);
            }
        }
        app
    }

    /// Loads a Vollded folder. Returns true on success.
    fn load_root(&mut self, root: PathBuf) -> bool {
        if !root.is_dir() {
            self.msg = format!("Not a folder: {}", root.display());
            return false;
        }
        self.items = scan(&root);
        self.meta = load_meta(&root);
        self.root_input = root.display().to_string();
        self.msg = format!("Loaded {} items from {}", self.items.len(), root.display());
        self.root = Some(root);
        self.selected = if self.items.is_empty() { None } else { Some(0) };
        self.rename_buf = None;
        self.thumb = None;
        self.drag_from = None;
        self.rebuild_view(); // refresh the visible list right now (see `reload`)
        true
    }

    /// Like `load_root`, but also remembers the folder in the config file.
    fn open_root(&mut self, root: PathBuf) {
        if self.load_root(root.clone()) {
            save_cfg_root(&root);
        }
    }

    /// Rescans the disk. `select` = the UGC id to select afterwards (else keep the current one).
    fn reload(&mut self, select: Option<String>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let old_idx = self.selected;
        let keep = select.or_else(|| {
            self.selected
                .and_then(|i| self.items.get(i))
                .map(|i| i.id.clone())
        });
        self.items = scan(&root);
        self.selected = keep.and_then(|k| self.items.iter().position(|i| i.id == k));
        // The selected UGC is gone (moved or deleted outside the app)? Select its neighbour.
        if self.selected.is_none() && !self.items.is_empty() {
            self.selected = Some(old_idx.unwrap_or(0).min(self.items.len() - 1));
        }
        self.thumb = None;
        self.rename_buf = None;
        self.drag_from = None;
        // Rebuild the visible list NOW: the list panel is drawn later in this same frame and
        // must never see indices of folders that no longer exist.
        self.rebuild_view();
    }

    /// Writes statuses + notes to `.ugc_manager.json`.
    fn save_meta(&mut self) {
        if let Some(root) = &self.root {
            if let Ok(s) = serde_json::to_string_pretty(&self.meta) {
                if fs::write(meta_path(root), s).is_err() {
                    self.msg = "Could not save metadata file.".into();
                }
            }
        }
        self.meta_save_at = None;
    }

    fn status_of(&self, id: &str) -> Status {
        self.meta.get(id).map(|m| m.status).unwrap_or_default()
    }

    /// Recomputes the filtered list and the totals. Only called when something changed.
    fn rebuild_view(&mut self) {
        let q = self.search.to_lowercase();
        self.visible = (0..self.items.len())
            .filter(|&i| {
                let it = &self.items[i];
                (q.is_empty() || it.title_lc.contains(&q) || it.id.contains(&q))
                    && self.filter.map_or(true, |f| self.status_of(&it.id) == f)
            })
            .collect();
        let total = self.items.len();
        let done = self.items.iter().filter(|i| i.progress() >= 1.0).count();
        let up = self
            .items
            .iter()
            .filter(|i| self.status_of(&i.id) == Status::Uploaded)
            .count();
        self.stats = (total, done, up);
        self.total_bytes = self.items.iter().map(|i| i.size_bytes).sum();
        self.view_dirty = false;
    }

    /// Starts decoding the reference image on a background thread (if not already loaded).
    fn ensure_thumb(&mut self, ctx: &egui::Context, want: Option<PathBuf>) {
        let Some(p) = want else {
            self.thumb = None;
            return;
        };
        if self.thumb.as_ref().map_or(false, |t| t.path == p) {
            return; // already loading / loaded
        }
        self.thumb = Some(Thumb {
            path: p.clone(),
            tex: None,
            done: false,
        });
        let tx = self.thumb_tx.clone();
        let ctx = ctx.clone();
        // `move` gives the closure ownership of the variables it uses (needed for threads).
        std::thread::spawn(move || {
            let img = image::open(&p).ok().map(|im| {
                let t = im.thumbnail(720, 720).to_rgba8(); // shrink to max 720px
                egui::ColorImage::from_rgba_unmultiplied(
                    [t.width() as usize, t.height() as usize],
                    t.as_raw(),
                )
            });
            let _ = tx.send((p, img)); // hand the result to the UI thread
            ctx.request_repaint(); // wake the UI up so it shows the image
        });
    }

    // ── 6b. Drag & drop: deciding what a drop means ──

    /// Called once with the files that were just dropped on the window.
    fn handle_drop(&mut self, ctx: &egui::Context, files: Vec<PathBuf>) {
        let Some(root) = self.root.clone() else {
            self.msg = "Pick your Vollded folder first.".into();
            return;
        };
        let pos = drop_pos(ctx);
        let on_list = pos.map_or(false, |p| self.list_rect.contains(p));
        let on_main = pos.map_or(false, |p| self.main_rect.contains(p));

        if on_list {
            // ── Dropped on the LEFT list → create a brand-new UGC folder ──
            let next = self
                .items
                .iter()
                .filter_map(|i| i.id.parse::<u64>().ok())
                .max()
                .unwrap_or(0)
                + 1;
            let id = next.to_string();
            let dir = root.join(&id);
            let (ops, rejected) = plan_ops(&id, &dir, None, &files);
            if ops.is_empty() {
                self.msg = format!("Nothing to add.{}", rejected_note(&rejected));
                return; // don't create an empty folder
            }
            self.apply_and_report(id, ops, rejected, true);
        } else if on_main {
            // ── Dropped on the UGC VIEW → add to the selected UGC ──
            let Some(sel) = self.selected.and_then(|i| self.items.get(i)) else {
                self.msg = "Select a UGC first, then drop files on it.".into();
                return;
            };
            let id = sel.id.clone();
            let (ops, rejected) = plan_ops(&id, &sel.dir.clone(), Some(sel), &files);
            if ops.is_empty() {
                self.msg = format!("Nothing to add.{}", rejected_note(&rejected));
                return;
            }
            if ops.iter().any(|o| o.existing.is_some()) {
                // Something would be overwritten → ask first (shown in `confirm_ui`).
                self.pending = Some(Pending { id, ops, rejected });
            } else {
                self.apply_and_report(id, ops, rejected, false);
            }
        } else {
            self.msg =
                "Drop onto the UGC list (new UGC) or the UGC view (add to the selected UGC)."
                    .into();
        }
    }

    /// Performs the copy, rescans, and tells the user what happened.
    fn apply_and_report(&mut self, id: String, ops: Vec<Op>, rejected: Vec<String>, is_new: bool) {
        match apply_ops(&ops) {
            Ok(_) => {
                self.reload(Some(id.clone()));
                let what = if is_new {
                    format!("Created UGC {id}")
                } else {
                    format!("Updated UGC {id}")
                };
                self.msg = format!(
                    "{what} with {} file(s).{}",
                    ops.len(),
                    rejected_note(&rejected)
                );
            }
            Err(e) => {
                self.reload(Some(id)); // show whatever did get copied
                self.msg = e;
            }
        }
    }

    /// The "Replace existing files?" pop-up.
    fn confirm_ui(&mut self, ctx: &egui::Context) {
        let mut decision: Option<bool> = None; // Some(true) = replace, Some(false) = cancel
        if let Some(p) = &self.pending {
            egui::Window::new("Replace existing files?")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0]) // centered in the window
                .show(ctx, |ui| {
                    ui.set_max_width(440.0);
                    ui.label(format!("UGC {} already has these files:", p.id));
                    ui.add_space(4.0);
                    for op in p.ops.iter().filter(|o| o.existing.is_some()) {
                        let old = op.existing.clone().unwrap_or_default();
                        ui.label(
                            RichText::new(format!(
                                "• {}: replace \"{}\" with \"{}\"",
                                op.label,
                                old,
                                fname(&op.src)
                            ))
                            .color(MUTED),
                        );
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Replaced files are moved to the Recycle Bin, not deleted.")
                            .color(MUTED),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new("Replace").fill(ACCENT)).clicked() {
                            decision = Some(true);
                        }
                        if ui.button("Cancel").clicked() {
                            decision = Some(false);
                        }
                    });
                });
        }
        match decision {
            Some(true) => {
                if let Some(p) = self.pending.take() {
                    // `take()` moves the value out and leaves `None` behind.
                    self.apply_and_report(p.id, p.ops, p.rejected, false);
                }
            }
            Some(false) => {
                self.pending = None;
                self.msg = "Drop cancelled, nothing was changed.".into();
            }
            None => {}
        }
    }

    // ── About pop-up ──
    fn about_ui(&mut self, ctx: &egui::Context) {
        let mut is_open = true; // the window's X button sets this to false
        egui::Window::new("About")
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(430.0);
                ui.label(RichText::new("Vollded UGC").size(24.0).strong().color(ACCENT));
                // Version is read from Cargo.toml at compile time.
                ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(MUTED));
                ui.add_space(2.0);
                ui.label("A portable manager for Vollded's Roblox UGC project folders: files, checklist, status and notes in one place.");
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label("Source code:");
                    if link_text(ui, GITHUB_URL, 13.0).on_hover_text("Open on GitHub").clicked() {
                        let _ = open::that(GITHUB_URL);
                    }
                });
                ui.separator();
                ui.label(RichText::new("Built with").strong());
                egui::Grid::new("about_components").num_columns(2).spacing([14.0, 4.0]).show(ui, |ui| {
                    for &(name, what) in COMPONENTS.iter() {
                        ui.label(name);
                        ui.label(RichText::new(what).size(12.5).color(MUTED));
                        ui.end_row();
                    }
                });
                ui.separator();
                ui.label(
                    RichText::new("Settings are stored in vollded_ugc.json next to the exe. Statuses and notes are stored in .ugc_manager.json inside the Vollded folder. Nothing is ever deleted permanently: replaced files and deleted UGCs go to the Recycle Bin.")
                        .size(12.0)
                        .color(MUTED),
                );
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            is_open = false;
        }
        self.show_about = is_open;
    }

    // ── "Set as Uploaded" pop-up: a link to the published UGC is required ──
    fn upload_dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(d) = self.upload_dialog.as_mut() else {
            return;
        };
        let mut decision: Option<bool> = None; // Some(true) = confirm, Some(false) = cancel
        egui::Window::new("Set as Uploaded")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.label("Paste the link to the published UGC to mark it as Uploaded.");
                ui.add_space(4.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut d.link)
                        .hint_text("https://www.roblox.com/catalog/…")
                        .desired_width(440.0),
                );
                let valid = valid_link(&d.link);
                // Green outline = valid link, red = not yet.
                ui.painter().rect_stroke(
                    r.rect.expand(2.0),
                    6.0,
                    Stroke::new(2.0, if valid { GOOD } else { ERR }),
                );
                if d.focus {
                    r.request_focus();
                    d.focus = false;
                }
                let enter =
                    r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && valid;
                let (hint, col) = if valid {
                    ("Link looks good.", GOOD)
                } else {
                    ("Enter a full link starting with https://", ERR)
                };
                ui.label(RichText::new(hint).size(12.0).color(col));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(valid, egui::Button::new("Set as Uploaded").fill(ACCENT))
                        .clicked()
                        || enter
                    {
                        decision = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(false);
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            decision = Some(false);
        }
        let (id, link) = (d.id.clone(), d.link.trim().to_string());
        match decision {
            Some(true) => {
                let m = self.meta.entry(id.clone()).or_default();
                m.status = Status::Uploaded;
                m.link = link;
                self.upload_dialog = None;
                self.view_dirty = true;
                self.save_meta();
                self.msg = format!("UGC {id} marked as Uploaded.");
            }
            Some(false) => self.upload_dialog = None,
            None => {}
        }
    }

    // ── Delete a whole UGC (to the Recycle Bin) ──

    /// The "Delete this UGC?" pop-up.
    fn delete_confirm_ui(&mut self, ctx: &egui::Context) {
        let Some(id) = self.pending_delete.clone() else {
            return;
        };
        let Some(item) = self.items.iter().find(|i| i.id == id) else {
            self.pending_delete = None; // it vanished (e.g. after a rescan)
            return;
        };
        let mut decision: Option<bool> = None;
        egui::Window::new("Delete this UGC?")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(440.0);
                ui.label(RichText::new(format!("{}  (folder {})", item.title, item.id)).strong());
                ui.label(format!(
                    "The whole folder ({}) will be moved to the Recycle Bin.",
                    item.size_label
                ));
                ui.label(
                    RichText::new(
                        "Nothing is deleted permanently, it can be restored from the Recycle Bin.",
                    )
                    .color(MUTED),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new("Move to Recycle Bin").fill(ACCENT))
                        .clicked()
                    {
                        decision = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        decision = Some(false);
                    }
                });
            });
        match decision {
            Some(true) => self.delete_ugc(&id),
            Some(false) => self.pending_delete = None,
            None => {}
        }
    }

    /// Moves the UGC's folder to the Recycle Bin and updates the list.
    fn delete_ugc(&mut self, id: &str) {
        self.pending_delete = None;
        let Some(pos) = self.items.iter().position(|i| i.id == id) else {
            return;
        };
        let dir = self.items[pos].dir.clone();
        let title = self.items[pos].title.clone();
        match to_recycle_bin(&dir) {
            Ok(_) => {
                self.meta.remove(id); // forget its status + notes
                self.save_meta();
                self.reload(None);
                // If the deleted one was selected, select its neighbour.
                if self.selected.is_none() && !self.items.is_empty() {
                    self.selected = Some(pos.min(self.items.len() - 1));
                }
                self.msg = format!("Moved UGC {id} ({title}) to the Recycle Bin.");
            }
            Err(e) => self.msg = e,
        }
    }

    // ── Reordering ──

    /// Reordering renames folders, so it is only allowed when the list is complete and numeric.
    fn reorder_allowed(&self) -> bool {
        self.search.is_empty()
            && self.filter.is_none()
            && self.items.iter().all(|i| i.id.parse::<u64>().is_ok())
    }

    /// Moves the card at index `from` to index `to`, then renumbers folders (and the files
    /// inside) so the number always matches the position: 1, 2, 3…
    fn reorder(&mut self, from: usize, to: usize) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let n = self.items.len();
        if from >= n || to >= n {
            return;
        }
        // New order as a list of old indices.
        let mut order: Vec<usize> = (0..n).collect();
        let moved = order.remove(from);
        order.insert(to, moved);

        // Which folders change number? (old id, new id)
        let mut plan: Vec<(String, String)> = vec![];
        for (k, &idx) in order.iter().enumerate() {
            let old = self.items[idx].id.clone();
            let new = (k + 1).to_string();
            if old != new {
                plan.push((old, new));
            }
        }
        if plan.is_empty() {
            return;
        }
        let sel_old = self
            .selected
            .and_then(|i| self.items.get(i))
            .map(|i| i.id.clone());

        match renumber_folders(&root, &plan) {
            Err(e) => self.msg = e, // nothing was changed
            Ok(problems) => {
                // Status + notes follow their UGC to the new number.
                let new_ids: HashSet<String> = plan.iter().map(|(_, new)| new.clone()).collect();
                let mut new_meta: HashMap<String, Meta> = HashMap::new();
                for (old, new) in &plan {
                    if let Some(m) = self.meta.remove(old) {
                        new_meta.insert(new.clone(), m);
                    }
                }
                for (k, v) in self.meta.drain() {
                    if !new_ids.contains(&k) {
                        new_meta.insert(k, v); // untouched UGCs keep theirs
                    }
                }
                self.meta = new_meta;
                self.save_meta();

                // Keep the same UGC selected (it now has a new number).
                let sel_new = sel_old.map(|o| {
                    plan.iter()
                        .find(|(a, _)| *a == o)
                        .map(|(_, b)| b.clone())
                        .unwrap_or(o)
                });
                self.reload(sel_new);
                self.msg = if problems.is_empty() {
                    "Reordered: folders and files were renamed to match the new order.".into()
                } else {
                    format!("Reordered, but: {}", problems.join("; "))
                };
            }
        }
    }

    /// Width the left panel needs so the size label ("12.3 MB") never gets cut off or overlaps.
    fn measure_side_min(ctx: &egui::Context) -> f32 {
        let width = |t: &str| {
            ctx.fonts(|f| {
                f.layout_no_wrap(t.to_owned(), FontId::proportional(12.0), Color32::WHITE)
                    .size()
                    .x
            })
        };
        let status_w = Status::ALL
            .iter()
            .map(|s| width(s.label()))
            .fold(0.0_f32, f32::max);
        let size_w = width("888.88 GB"); // the widest size text that can appear
                                         // badge+title offset 68 · dot indent 14 · status · gap 12 · size · right padding 10
                                         // + panel margins 20 + scroll bar 16
        68.0 + 14.0 + status_w + 12.0 + size_w + 10.0 + 20.0 + 16.0
    }

    // ── Top bar ──
    // Everything is laid out RIGHT-TO-LEFT in one row: stats (far right), Rescan, Browse, then the
    // path box takes whatever width is left, and the title sits at the far left. That way the
    // buttons and stats stay in the same place at every window width, even the smallest.
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let (total, done, up) = self.stats;
        // Exact width of the title, measured with the real font.
        let title_w = ui.fonts(|f| {
            f.layout_no_wrap(
                "Vollded UGC".to_owned(),
                FontId::proportional(20.0),
                Color32::WHITE,
            )
            .size()
            .x
        });

        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Added first = ends up at the far right.
                ui.label(RichText::new(format!("{up} uploaded")).color(ACCENT));
                ui.label(RichText::new("·").color(MUTED));
                ui.label(RichText::new(format!("{done}/{total} complete")).color(GOOD));
                ui.label(RichText::new("·").color(MUTED));
                ui.label(RichText::new(format!("{total} items")).color(MUTED));
                ui.add_space(8.0);

                if ui.button("Rescan").clicked() {
                    self.reload(None);
                    self.msg = "Rescanned folders.".into();
                }
                if ui.button("Browse").clicked() {
                    if let Some(p) = rfd::FileDialog::new().pick_folder() {
                        self.open_root(p);
                    }
                }

                // The path box fills all the space that is left (minus the title).
                let path_w = (ui.available_width() - title_w - 24.0).max(80.0);
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.root_input)
                        .hint_text("Path to Vollded folder…")
                        .desired_width(path_w),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.open_root(PathBuf::from(self.root_input.trim()));
                }

                // Added last = leftmost.
                ui.label(
                    RichText::new("Vollded UGC")
                        .size(20.0)
                        .strong()
                        .color(ACCENT),
                );
            });
        });
    }

    // ── Left list (search, status dropdown, cards) ──
    fn list_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search…")
                .desired_width(f32::INFINITY),
        );

        // Status filter as a drop-down menu.
        egui::ComboBox::from_id_salt("status_filter")
            .selected_text(self.filter.map_or("All statuses", |s| s.label()))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.filter, None, "All statuses");
                for s in Status::ALL {
                    ui.selectable_value(&mut self.filter, Some(s), s.label());
                }
            });

        let reorder_ok = self.reorder_allowed();
        let mut drop_target: Option<usize> = None; // card the dragged card is currently over
        let pointer = ui.input(|i| i.pointer.interact_pos());
        if self.drag_from.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        }

        // `show_rows` only draws the rows that are on screen (fast even with 1000 UGCs).
        let n = self.visible.len();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, 70.0, n, |ui, range| {
            for row in range {
                // `.get()` instead of `[ ]`: returns None instead of crashing if the list is out of date.
                let Some(&i) = self.visible.get(row) else { continue };
                if i >= self.items.len() {
                    continue;
                }
                // The card reacts to clicks (select) AND drags (reorder).
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), 70.0), Sense::click_and_drag());

                // Delete button: registered AFTER the card, so it sits on top and gets its own clicks.
                let id = self.items[i].id.clone();
                let del_rect = Rect::from_min_size(egui::pos2(rect.max.x - 32.0, rect.min.y + 6.0), Vec2::splat(24.0));
                let del = ui.interact(del_rect, egui::Id::new(("ugc_delete", &id)), Sense::click());
                let del_hover = del.hovered();
                let del_clicked = del.clicked();
                del.on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text("Delete this UGC (moves the folder to the Recycle Bin)");

                // Start of a drag = start of a reorder.
                if resp.drag_started() {
                    if reorder_ok {
                        self.drag_from = Some(i);
                    } else {
                        self.msg = "To reorder: clear the search and status filter (and all folders must be numbered).".into();
                    }
                }
                // While dragging: which card is the pointer over?
                if let (true, Some(pos)) = (self.drag_from.is_some(), pointer) {
                    let band = Rect::from_min_max(
                        egui::pos2(rect.min.x, rect.min.y - 5.0),
                        egui::pos2(rect.max.x, rect.max.y + 5.0),
                    );
                    if band.contains(pos) {
                        drop_target = Some(i);
                    }
                }

                let it = &self.items[i];
                let st = self.status_of(&it.id);
                let prog = it.progress();
                let sel = self.selected == Some(i);

                // Card background (dimmed if being dragged, highlighted if selected / hovered).
                let fill = if self.drag_from == Some(i) {
                    CARD.gamma_multiply(0.5)
                } else if sel {
                    ACCENT.gamma_multiply(0.20)
                } else if resp.hovered() {
                    CARD_HOVER
                } else {
                    CARD
                };
                let p = ui.painter_at(rect); // a painter clipped to this card
                p.rect_filled(rect, 10.0, fill);
                if sel {
                    p.rect_stroke(rect, 10.0, Stroke::new(1.5, ACCENT));
                }

                // Number badge on the left.
                let badge = Rect::from_min_size(rect.min + Vec2::new(10.0, 12.0), Vec2::splat(46.0));
                p.rect_filled(badge, 10.0, BG);
                p.text(badge.center(), Align2::CENTER_CENTER, &it.id, FontId::proportional(18.0), ACCENT);

                // Title (cut off with "…"); leaves room on the right for the delete icon.
                let tx = rect.min.x + 68.0;
                let tw = (rect.max.x - tx - 10.0).max(10.0);
                ellipsized(&p, egui::pos2(tx, rect.min.y + 10.0), &it.title, FontId::proportional(14.5), TEXT, (tw - 30.0).max(10.0));

                // Delete icon (trash can).
                if del_hover {
                    p.rect_filled(del_rect, 6.0, BG);
                }
                draw_trash(&p, del_rect.center(), if del_hover { ERR } else { MUTED });

                // Status dot + label, and the folder size on the right (always shown: the
                // panel can't get narrower than this needs, see `measure_side_min`).
                p.circle_filled(egui::pos2(tx + 4.0, rect.min.y + 39.0), 4.0, st.color());
                p.text(egui::pos2(tx + 14.0, rect.min.y + 32.0), Align2::LEFT_TOP, st.label(), FontId::proportional(12.0), MUTED);
                p.text(
                    egui::pos2(rect.max.x - 10.0, rect.min.y + 32.0),
                    Align2::RIGHT_TOP,
                    &it.size_label,
                    FontId::proportional(12.0),
                    MUTED,
                );

                // Mini progress bar along the bottom.
                let bar = Rect::from_min_size(egui::pos2(tx, rect.min.y + 55.0), Vec2::new(tw, 4.0));
                p.rect_filled(bar, 2.0, BG);
                let col = if prog >= 1.0 { GOOD } else { WARN };
                p.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * prog, 4.0)), 2.0, col);

                // Drop indicator: a red line where the dragged card would land.
                if let Some(from) = self.drag_from {
                    if drop_target == Some(i) && from != i {
                        let y = if i > from { rect.max.y + 5.0 } else { rect.min.y - 5.0 };
                        ui.painter().line_segment(
                            [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                            Stroke::new(3.0, ACCENT),
                        );
                    }
                }

                if del_clicked {
                    self.pending_delete = Some(id); // ask for confirmation first
                } else if resp.clicked() {
                    self.selected = Some(i);
                    self.rename_buf = None;
                }
            }
        });

        // Drag finished (mouse released) or cancelled (Esc)?
        if self.drag_from.is_some() {
            let released = !ui.input(|i| i.pointer.any_down());
            let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if released || esc {
                let from = self.drag_from.take();
                if released && !esc {
                    if let (Some(from), Some(to)) = (from, drop_target) {
                        if from != to {
                            self.reorder(from, to); // renames folders + files on disk
                        }
                    }
                }
            }
        }
    }

    // ── Detail view of the selected UGC ──
    fn detail_ui(&mut self, ui: &mut egui::Ui, idx: usize) {
        let ctx = ui.ctx().clone();

        // Make sure the reference image is (being) loaded for this UGC.
        let same = match (&self.items[idx].reference, &self.thumb) {
            (Some(p), Some(t)) => &t.path == p,
            (None, None) => true,
            _ => false,
        };
        if !same {
            let want = self.items[idx].reference.clone();
            self.ensure_thumb(&ctx, want);
        }

        // Borrow different fields of `self` separately. Rust allows several `&mut` borrows
        // at once as long as they point at DIFFERENT fields.
        let thumb = self.thumb.as_ref();
        let item = &mut self.items[idx];
        // Copy the id now: `item` must not be used after `self.save_meta()` below (that call borrows all of `self`).
        let item_id = item.id.clone();
        let meta = self.meta.entry(item_id.clone()).or_default();
        let rename_buf = &mut self.rename_buf;
        let rename_focus = &mut self.rename_focus;
        // Instead of changing `self` while we're still drawing, UI code records what the
        // user wanted in `act`, and we carry it out after drawing is finished.
        let mut act = Actions::default();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // ── Header: name + Rename ──
                if let Some(buf) = rename_buf.as_mut() {
                    // Rename mode. You may type more than 50 characters, but the outline turns
                    // red and Save is disabled until the name is valid (1–50 characters).
                    let count = buf.chars().count();
                    let valid = count <= MAX_NAME && !buf.trim().is_empty();
                    let r = ui.add(
                        egui::TextEdit::singleline(buf)
                            .font(FontId::proportional(22.0))
                            .desired_width(f32::INFINITY),
                    );
                    // Draw a colored outline on top of the text box: green = valid, red = invalid.
                    ui.painter().rect_stroke(
                        r.rect.expand(2.0),
                        6.0,
                        Stroke::new(2.0, if valid { GOOD } else { ERR }),
                    );
                    if *rename_focus {
                        r.request_focus(); // put the cursor in the box the first time
                        *rename_focus = false;
                    }
                    let enter =
                        r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && valid;
                    let mut cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(valid, egui::Button::new("Save name").fill(ACCENT))
                            .clicked()
                            || enter
                        {
                            act.rename = Some(buf.clone());
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                        let counter_color = if count > MAX_NAME { ERR } else { MUTED };
                        ui.label(
                            RichText::new(format!("{count}/{MAX_NAME}"))
                                .size(12.0)
                                .color(counter_color),
                        );
                        if count > MAX_NAME {
                            ui.label(
                                RichText::new("Too long: shorten it to save")
                                    .size(12.0)
                                    .color(ERR),
                            );
                        }
                    });
                    if cancel {
                        *rename_buf = None;
                    }
                } else {
                    // Normal mode: show the title and the Rename / Open folder buttons.
                    ui.add(
                        egui::Label::new(RichText::new(&item.title).size(26.0).strong())
                            .wrap_mode(egui::TextWrapMode::Wrap),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Rename").clicked() {
                            *rename_buf = Some(item.title.clone());
                            *rename_focus = true;
                        }
                        if ui.button("Open folder").clicked() {
                            act.to_open = Some(item.dir.clone());
                        }
                        // Link to the published UGC (only for UGCs set to Uploaded).
                        if meta.status == Status::Uploaded && !meta.link.is_empty() {
                            if ui
                                .add(
                                    egui::Button::new("Open published UGC")
                                        .fill(ACCENT.gamma_multiply(0.6)),
                                )
                                .clicked()
                            {
                                act.open_url = Some(meta.link.clone());
                            }
                        }
                    });
                }

                // ── Status chips ──
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                    ui.label(RichText::new("Status").color(MUTED));
                    for s in Status::ALL {
                        let sel = meta.status == s;
                        let txt_col = if !sel {
                            TEXT
                        } else if s == Status::Uploaded {
                            Color32::WHITE
                        } else {
                            BG
                        };
                        let b = egui::Button::new(RichText::new(s.label()).color(txt_col))
                            .fill(if sel { s.color() } else { CARD_HOVER })
                            .rounding(20.0);
                        if ui.add(b).clicked() {
                            if s == Status::Uploaded {
                                // Uploaded needs a link: open the pop-up (prefilled with the old link).
                                act.ask_uploaded = Some(meta.link.clone());
                            } else if !sel {
                                meta.status = s;
                                act.status_changed = true;
                            }
                        }
                    }
                });

                // ── Body: always 2 columns (the cards just get narrower in a small window) ──
                ui.columns(2, |cols| {
                    left_col(&mut cols[0], item, meta, &mut act, thumb);
                    right_col(&mut cols[1], item, &mut act);
                });
            });

        // ── Now carry out whatever the user asked for while we were drawing ──
        if act.save_desc {
            if let Some(t) = &item.txt {
                match fs::write(t, &item.description) {
                    Ok(_) => {
                        item.dirty = false;
                        item.desc_saved = !item.description.trim().is_empty();
                        self.view_dirty = true;
                        self.msg = "Description saved.".into();
                    }
                    Err(e) => self.msg = format!("Save failed: {e}"),
                }
            }
        }
        if let Some(new) = act.rename.take() {
            match do_rename(item, &new) {
                Ok(_) => {
                    self.rename_buf = None;
                    self.view_dirty = true;
                    self.msg = format!("Renamed to \"{}\".", item.title);
                }
                Err(e) => self.msg = e,
            }
        }
        if act.status_changed {
            self.view_dirty = true;
            self.save_meta();
        }
        if act.notes_edited {
            self.meta_save_at = Some(Instant::now()); // saved ~1s later, see update()
        }
        if let Some(d) = act.open_dir {
            let _ = fs::create_dir_all(&d); // create previews/ or textures/ if it doesn't exist yet
            if let Err(e) = open::that(&d) {
                self.msg = format!("Could not open {}: {e}", d.display());
            }
        }
        if let Some(link) = act.ask_uploaded.take() {
            self.upload_dialog = Some(UploadDialog {
                id: item_id,
                link,
                focus: true,
            });
        }
        if let Some(u) = act.open_url {
            if let Err(e) = open::that(&u) {
                self.msg = format!("Could not open the link: {e}");
            }
        }
        if let Some(p) = act.to_open {
            if let Err(e) = open::that(&p) {
                self.msg = format!("Could not open {}: {e}", p.display());
            }
        }
    }
}

// ───────────────────────── 7. Small UI helpers ─────────────────────────

/// What the user did during one frame (filled in by the UI code, handled afterwards).
#[derive(Default)]
struct Actions {
    to_open: Option<PathBuf>,  // open this file with its default app
    open_dir: Option<PathBuf>, // open this folder in Explorer
    save_desc: bool,
    status_changed: bool,
    notes_edited: bool,
    rename: Option<String>,
    ask_uploaded: Option<String>, // open the "Set as Uploaded" pop-up, prefilled with this link
    open_url: Option<String>,     // open this link in the browser
}

/// A rounded "card" container with a plain title. `body` is a closure (an inline function)
/// that draws the card's contents.
fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(CARD)
        .rounding(12.0)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(title).size(15.0).strong());
            ui.add_space(2.0);
            body(ui);
        });
}

/// Like `card`, but the title is clickable and ends with a white ">" arrow. Returns true if clicked.
fn card_link(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let mut clicked = false;
    egui::Frame::none()
        .fill(CARD)
        .rounding(12.0)
        .inner_margin(14.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            // Lay the title text out first so we know how wide the clickable area must be.
            let galley =
                ui.painter()
                    .layout_no_wrap(title.to_owned(), FontId::proportional(15.0), TEXT);
            let size = Vec2::new(galley.size().x + 24.0, galley.size().y.max(18.0));
            let (rect, r) = ui.allocate_exact_size(size, Sense::click());
            let r = r
                .on_hover_cursor(CursorIcon::PointingHand) // hand cursor = "this is a link"
                .on_hover_text("Open this folder");
            let p = ui.painter();
            p.galley(rect.min, galley, TEXT);
            // The arrow ">" drawn as two lines.
            let c = egui::pos2(rect.max.x - 7.0, rect.center().y);
            let st = Stroke::new(1.8, Color32::WHITE);
            p.line_segment([c + Vec2::new(-3.0, -5.0), c + Vec2::new(2.0, 0.0)], st);
            p.line_segment([c + Vec2::new(2.0, 0.0), c + Vec2::new(-3.0, 5.0)], st);
            if r.clicked() {
                clicked = true;
            }
            ui.add_space(2.0);
            body(ui);
        });
    clicked
}

/// Grey italic placeholder text.
fn empty_hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).italics().color(MUTED));
}

/// A label that is cut off with "…" when it doesn't fit in `width`.
fn trunc_label(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, 20.0),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_size(Vec2::new(width, 20.0));
            ui.set_max_width(width);
            ui.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Truncate));
        },
    );
}

/// Paints single-line text that is shortened with "…" to fit `max_w` (used on list cards).
fn ellipsized(
    p: &egui::Painter,
    pos: egui::Pos2,
    text: &str,
    font: FontId,
    color: Color32,
    max_w: f32,
) {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap.max_width = max_w;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let g = p.layout_job(job);
    p.galley(pos, g, color);
}

/// Progress bar with always-readable text: white text with a dark outline on a darker fill.
fn progress_bar(ui: &mut egui::Ui, prog: f32, text: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 6.0, BG); // empty track
    let fill = if prog >= 1.0 {
        Color32::from_rgb(14, 140, 95)
    } else {
        Color32::from_rgb(170, 110, 6)
    };
    let w = rect.width() * prog;
    if w > 0.0 {
        p.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(w, rect.height())),
            6.0,
            fill,
        );
    }
    let font = FontId::proportional(13.5);
    // Draw the text 4 times in black, shifted 1px each way, to make an outline…
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        p.text(
            rect.center() + Vec2::new(dx, dy),
            Align2::CENTER_CENTER,
            text,
            font.clone(),
            Color32::from_black_alpha(220),
        );
    }
    // …then once in white on top.
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        font,
        Color32::WHITE,
    );
}

/// Small "GitHub" link with a hand-drawn git-branch icon. Returns the click response.
fn github_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(78.0, 20.0), Sense::click());
    let col = if resp.hovered() { ACCENT } else { MUTED };
    let p = ui.painter();
    let c = rect.min + Vec2::new(10.0, 10.0); // icon center
    let stroke = Stroke::new(1.3, col);
    p.circle_stroke(c, 9.0, stroke); // outer ring
    let top = c + Vec2::new(-2.5, -4.0);
    let bottom = c + Vec2::new(-2.5, 4.0);
    let branch = c + Vec2::new(3.0, -1.0);
    p.line_segment([top, bottom], stroke); // main line
    p.line_segment([c + Vec2::new(-2.5, 2.0), branch], stroke); // side branch
    for n in [top, bottom, branch] {
        p.circle_filled(n, 1.7, col); // the three commit dots
    }
    p.text(
        egui::pos2(rect.min.x + 24.0, rect.center().y),
        Align2::LEFT_CENTER,
        "GitHub",
        FontId::proportional(12.5),
        col,
    );
    resp.on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text("Open the Vollded page on GitHub")
}

/// Small round "i" button (drawn with the painter, no icon font needed).
fn info_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
    let col = if resp.hovered() { ACCENT } else { MUTED };
    let p = ui.painter();
    let c = rect.center();
    p.circle_stroke(c, 8.5, Stroke::new(1.3, col)); // ring
    p.circle_filled(c + Vec2::new(0.0, -3.8), 1.2, col); // dot of the "i"
    p.line_segment(
        [c + Vec2::new(0.0, -1.0), c + Vec2::new(0.0, 4.6)],
        Stroke::new(1.7, col),
    ); // stem
    resp.on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text("About Vollded UGC")
}

/// Small clickable text (turns red + underlined on hover). Returns the click response.
fn link_text(ui: &mut egui::Ui, text: &str, size: f32) -> egui::Response {
    // Color32::PLACEHOLDER = "use the color I pass to `galley()` later", so hover can change it.
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        FontId::proportional(size),
        Color32::PLACEHOLDER,
    );
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::click());
    let col = if resp.hovered() { ACCENT } else { MUTED };
    let p = ui.painter();
    p.galley(rect.min, galley, col);
    if resp.hovered() {
        p.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0, col),
        );
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Draws a little trash-can icon centered on `c` (no icon font needed).
fn draw_trash(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    let s = Stroke::new(1.4, col);
    p.line_segment([c + Vec2::new(-6.0, -4.0), c + Vec2::new(6.0, -4.0)], s); // lid
    p.line_segment([c + Vec2::new(-2.0, -4.0), c + Vec2::new(-2.0, -6.5)], s); // handle
    p.line_segment([c + Vec2::new(2.0, -4.0), c + Vec2::new(2.0, -6.5)], s);
    p.line_segment([c + Vec2::new(-2.0, -6.5), c + Vec2::new(2.0, -6.5)], s);
    p.rect_stroke(
        Rect::from_min_max(c + Vec2::new(-4.5, -3.0), c + Vec2::new(4.5, 6.5)),
        1.5,
        s,
    ); // body
    p.line_segment([c + Vec2::new(-1.5, -0.5), c + Vec2::new(-1.5, 4.0)], s); // slats
    p.line_segment([c + Vec2::new(1.5, -0.5), c + Vec2::new(1.5, 4.0)], s);
}

/// Highlights a drop zone while files are being dragged over the window.
fn draw_drop_zone(p: &egui::Painter, rect: Rect, text: &str) {
    if rect.width() < 10.0 {
        return;
    }
    let r = rect.shrink(6.0);
    p.rect_filled(r, 14.0, Color32::from_rgba_unmultiplied(225, 48, 60, 40));
    p.rect_stroke(r, 14.0, Stroke::new(2.0, ACCENT));
    p.text(
        r.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::proportional(16.0),
        TEXT,
    );
}

/// Left column of the detail view: Reference, Description, Notes.
fn left_col(
    ui: &mut egui::Ui,
    item: &mut Item,
    meta: &mut Meta,
    act: &mut Actions,
    thumb: Option<&Thumb>,
) {
    card(ui, "Reference", |ui| {
        if let Some(p) = &item.reference {
            // `match` on a tuple: (texture if any, finished-loading flag).
            match thumb.map(|t| (t.tex.as_ref(), t.done)) {
                Some((Some(tex), _)) => {
                    let w = ui.available_width();
                    let r = ui.add(
                        egui::Image::from_texture(SizedTexture::from_handle(tex))
                            .max_size(Vec2::new(w, 300.0))
                            .rounding(10.0)
                            .sense(Sense::click()),
                    );
                    if r.clicked() {
                        act.to_open = Some(p.clone()); // click the image to open the original
                    }
                }
                Some((None, true)) => empty_hint(ui, "Preview unavailable"),
                _ => empty_hint(ui, "Loading…"),
            }
            ui.label(RichText::new(fname(p)).size(12.0).color(MUTED));
        } else {
            empty_hint(ui, "No reference image found (drop a .jpg here)");
        }
    });

    card(ui, "Description", |ui| {
        if item.txt.is_none() {
            empty_hint(ui, "No .txt yet. Use Rename (or drop a .txt) to create it.");
        } else {
            let r = ui.add(
                egui::TextEdit::multiline(&mut item.description)
                    .desired_width(f32::INFINITY)
                    .desired_rows(7),
            );
            if r.changed() {
                item.dirty = true;
            }
            let count = item.description.chars().count();
            let over = count > MAX_DESC;
            // Outline: red when too long, green while editing a valid text (hidden otherwise).
            if over || item.dirty || r.has_focus() {
                ui.painter().rect_stroke(
                    r.rect.expand(2.0),
                    6.0,
                    Stroke::new(2.0, if over { ERR } else { GOOD }),
                );
            }
            ui.horizontal_wrapped(|ui| {
                let can_save = item.dirty && !over;
                let b = egui::Button::new("Save to .txt").fill(if can_save {
                    ACCENT
                } else {
                    CARD_HOVER
                });
                if ui.add_enabled(can_save, b).clicked() {
                    act.save_desc = true;
                }
                let col = if over {
                    ERR
                } else if item.dirty {
                    GOOD
                } else {
                    MUTED
                };
                ui.label(
                    RichText::new(format!("{count}/{MAX_DESC}"))
                        .size(12.0)
                        .color(col),
                );
                if over {
                    ui.label(
                        RichText::new("Too long: shorten it to save")
                            .size(12.0)
                            .color(ERR),
                    );
                }
            });
        }
    });

    card(ui, "Notes", |ui| {
        let r = ui.add(
            egui::TextEdit::multiline(&mut meta.notes)
                .hint_text("Private notes (polycount, fixes needed, upload date…)")
                .desired_width(f32::INFINITY)
                .desired_rows(4),
        );
        if r.changed() {
            act.notes_edited = true;
        }
    });
}

/// Right column of the detail view: Checklist, Previews, Textures.
fn right_col(ui: &mut egui::Ui, item: &Item, act: &mut Actions) {
    // ── Checklist ──
    card(ui, "Checklist", |ui| {
        // (label, file path if any, is it done?)
        let rows: [(&str, Option<&PathBuf>, bool); 5] = [
            (
                "Reference image",
                item.reference.as_ref(),
                item.reference.is_some(),
            ),
            ("Model", item.raw_blend.as_ref(), item.raw_blend.is_some()),
            ("Texture", item.spp.as_ref(), item.spp.is_some()),
            (
                "Ready",
                item.ready_blend.as_ref(),
                item.ready_blend.is_some(),
            ),
            ("Description", item.txt.as_ref(), item.desc_saved),
        ];
        for (label, path, ok) in rows {
            ui.horizontal(|ui| {
                if ok {
                    ui.label(RichText::new("✔").color(GOOD));
                } else {
                    ui.label(RichText::new("✖").color(BAD));
                }
                ui.label(label);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(p) = path {
                        if ui.small_button("Open").clicked() {
                            act.to_open = Some(p.clone());
                        }
                        if label != "Description" {
                            ui.label(RichText::new(item.size(p)).size(12.0).color(MUTED));
                        }
                    }
                });
            });
        }
        ui.add_space(2.0);
        let prog = item.progress();
        progress_bar(ui, prog, &format!("{:.0}% complete", prog * 100.0));
    });

    // ── Previews: the card title is a link (with a > arrow) to the previews folder ──
    let title = format!("Previews ({})", item.previews.len());
    let clicked = card_link(ui, &title, |ui| {
        if item.previews.is_empty() {
            empty_hint(ui, "No videos yet (drop an .mp4 here)");
        }
        for p in &item.previews {
            ui.horizontal(|ui| {
                // Name on the left (cut off with "…" if needed); size + play button on the right.
                let name_w = (ui.available_width() - 112.0).max(30.0);
                trunc_label(ui, &fname(p), name_w);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("▶").clicked() {
                        act.to_open = Some(p.clone());
                    }
                    ui.label(RichText::new(item.size(p)).size(12.0).color(MUTED));
                });
            });
        }
    });
    if clicked {
        act.open_dir = Some(item.dir.join("previews"));
    }

    // ── Textures: same idea. Every file is listed in full (no inner scroll box). ──
    let title = format!("Textures ({})", item.textures.len());
    let clicked = card_link(ui, &title, |ui| {
        if item.textures.is_empty() {
            empty_hint(ui, "No textures yet (drop .png files here)");
        }
        for p in &item.textures {
            ui.horizontal(|ui| {
                let name_w = (ui.available_width() - 72.0).max(30.0);
                trunc_label(ui, &fname(p), name_w);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("Open").clicked() {
                        act.to_open = Some(p.clone());
                    }
                });
            });
        }
    });
    if clicked {
        act.open_dir = Some(item.dir.join("textures"));
    }
}

// ───────────────────────── 8. update() and main() ─────────────────────────

/// `eframe::App` is a "trait" (an interface). Implementing it means: "this struct can be
/// run as an eframe application". `update` is called every time the window needs redrawing.
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 0) FRAME LIMITER (saves CPU). egui redraws on every mouse move, which can mean hundreds
        //    of redraws per second. We make each frame wait until at least 16 ms (scrolling /
        //    dragging, ~60 fps) or 32 ms (just hovering, ~30 fps) passed since the last one.
        let busy = ctx.input(|i| i.smooth_scroll_delta != Vec2::ZERO || i.pointer.any_down());
        let min_frame = Duration::from_millis(if busy { 16 } else { 32 });
        let since = self.last_frame.elapsed();
        if since < min_frame {
            std::thread::sleep(min_frame - since); // sleeping uses no CPU
        }
        self.last_frame = Instant::now();

        // Measure the narrowest allowed left panel once (fonts exist from the first frame on).
        if self.side_min_w == 0.0 {
            self.side_min_w = App::measure_side_min(ctx);
        }

        // 1) Pick up reference images that the background thread finished decoding.
        while let Ok((path, img)) = self.thumb_rx.try_recv() {
            if let Some(t) = self.thumb.as_mut() {
                if t.path == path {
                    t.done = true;
                    t.tex = img.map(|i| ctx.load_texture("ref", i, egui::TextureOptions::LINEAR));
                }
            }
        }

        // 2) Files dropped onto the window this frame?
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if !dropped.is_empty()
            && self.pending.is_none()
            && self.pending_delete.is_none()
            && self.upload_dialog.is_none()
        {
            self.handle_drop(ctx, dropped);
        }

        // 3) Rebuild the cached list only if the search/filter changed or data changed.
        if self.search != self.last_search || self.filter != self.last_filter {
            self.last_search = self.search.clone();
            self.last_filter = self.filter;
            self.view_dirty = true;
        }
        if self.view_dirty {
            self.rebuild_view();
        }

        // 4) Draw the panels. Order matters: top/bottom bars first, then the sides, then the center.
        egui::TopBottomPanel::top("top")
            .frame(
                egui::Frame::none()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(14.0, 10.0)),
            )
            .show(ctx, |ui| self.top_bar(ui));

        // Bottom bar: message on the left; folder size + GitHub link on the right.
        egui::TopBottomPanel::bottom("status")
            .exact_height(28.0)
            .frame(
                egui::Frame::none()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(16.0, 4.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let msg_w = (ui.available_width() - 272.0).max(50.0);
                    trunc_label(ui, &self.msg, msg_w);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Far right: the About (info) button.
                        if info_button(ui).clicked() {
                            self.show_about = true;
                        }
                        if github_button(ui).clicked() {
                            let _ = open::that(GITHUB_URL);
                        }
                        // Clickable: opens the Vollded folder in Explorer.
                        let txt = format!("Folder size: {}", fmt_bytes(self.total_bytes));
                        if link_text(ui, &txt, 12.0)
                            .on_hover_text("Open the Vollded folder")
                            .clicked()
                        {
                            match &self.root {
                                Some(r) => {
                                    if let Err(e) = open::that(r) {
                                        self.msg = format!("Could not open the folder: {e}");
                                    }
                                }
                                None => self.msg = "Pick your Vollded folder first.".into(),
                            }
                        }
                    });
                });
            });

        // The top bar may have just changed the items (Rescan / Browse). Refresh the cached list
        // NOW, before the list is drawn: otherwise it would use out-of-date indices (pointing at
        // folders that no longer exist) and the program would crash.
        if self.view_dirty {
            self.rebuild_view();
        }

        // `.show()` returns the area it used; we remember it so we know where drops land.
        // The panel can't be narrower than `side_min_w`: right before the size label would disappear.
        self.list_rect = egui::SidePanel::left("list")
            .default_width(self.side_min_w)
            .width_range(self.side_min_w..=440.0)
            .frame(egui::Frame::none().fill(PANEL).inner_margin(10.0))
            .show(ctx, |ui| self.list_ui(ui))
            .response
            .rect;

        self.main_rect = egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(16.0))
            .show(ctx, |ui| match self.selected {
                Some(i) if i < self.items.len() => self.detail_ui(ui, i),
                _ => {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Select a UGC, or drop files on the list to create one")
                                .size(16.0)
                                .color(MUTED),
                        );
                    });
                }
            })
            .response
            .rect;

        // 5) The "Replace existing files?" pop-up (only exists while a drop is pending).
        if self.pending.is_some() {
            self.confirm_ui(ctx);
        }
        if self.pending_delete.is_some() {
            self.delete_confirm_ui(ctx);
        }
        if self.upload_dialog.is_some() {
            self.upload_dialog_ui(ctx);
        }
        if self.show_about {
            self.about_ui(ctx);
        }

        // 6) While files are being dragged over the window, show where they can be dropped.
        if ctx.input(|i| !i.raw.hovered_files.is_empty())
            && self.pending.is_none()
            && self.pending_delete.is_none()
            && self.upload_dialog.is_none()
        {
            let p = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("drop_overlay"),
            ));
            draw_drop_zone(&p, self.list_rect, "Drop here\nto create a new UGC");
            draw_drop_zone(
                &p,
                self.main_rect,
                "Drop here to add to this UGC\n(.blend .spp .jpg .txt .mp4 .png)",
            );
        }

        // 7) Notes are saved ~1 second after you stop typing (and when the window closes).
        if let Some(t) = self.meta_save_at {
            let wait = Duration::from_millis(900);
            if ctx.input(|i| i.viewport().close_requested()) || t.elapsed() >= wait {
                self.save_meta();
            } else {
                ctx.request_repaint_after(wait - t.elapsed()); // wake up again when it's time
            }
        }
    }
}

/// Program entry point.
fn main() -> eframe::Result<()> {
    // If the app ever panics, write the reason next to the exe (no console window to read it from).
    std::panic::set_hook(Box::new(|info| {
        let _ = fs::write(exe_dir().join("vollded_ugc_crash.log"), format!("{info}\n"));
    }));
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([980.0, 600.0])
        .with_min_inner_size([980.0, 600.0])
        .with_title("Vollded UGC");
    // Window + taskbar icon (baked into the exe from assets/icon.png).
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        // Windowed apps are composed by the OS, so there is no visible tearing; turning vsync
        // off stops the graphics driver from busy-waiting. The frame limiter paces frames instead.
        vsync: false,
        ..Default::default() // every other option keeps its default value
    };
    // `Box::new(|cc| …)` is the function eframe calls to build our `App`.
    eframe::run_native(
        "Vollded UGC",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
