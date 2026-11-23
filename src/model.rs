//! The data types: a UGC's progress `Status`, the saved `Meta` (status, notes, published
//! link) and the `Item` that is built by scanning one UGC folder.

use eframe::egui::Color32;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use crate::theme::{ACCENT, GOOD, MUTED, WARN};
use crate::util::{dir_size, fmt_bytes, list_files, list_files_recursive};

/// The progress state of one UGC. An `enum` is "one of these variants".
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
pub enum Status {
    #[default] // `Status::default()` gives Todo
    Todo,
    Modeling,
    Texturing,
    Ready,
    Uploaded,
}

impl Status {
    /// All variants in display order (a fixed-size array: `[Type; length]`).
    pub const ALL: [Status; 5] = [
        Status::Todo,
        Status::Modeling,
        Status::Texturing,
        Status::Ready,
        Status::Uploaded,
    ];

    /// Text shown in the UI. `match` must cover every variant (the compiler checks!).
    pub fn label(self) -> &'static str {
        match self {
            Status::Todo => "To do",
            Status::Modeling => "Modeling",
            Status::Texturing => "Texturing",
            Status::Ready => "Ready",
            Status::Uploaded => "Uploaded",
        }
    }

    /// Color of the little status dot / chip.
    pub fn color(self) -> Color32 {
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
pub struct Meta {
    pub status: Status,
    pub notes: String,
    /// Link to the published UGC. Required to set the status to Uploaded.
    /// `#[serde(default)]` = old save files without this field still load fine.
    #[serde(default)]
    pub link: String,
}

/// Everything we know about ONE UGC folder (e.g. folder "1"), filled in by `Item::load`.
#[derive(Default)]
pub struct Item {
    pub id: String,                  // the folder name, e.g. "1"
    pub dir: PathBuf,                // full path of the folder
    pub raw_blend: Option<PathBuf>,  // "Model"  (`Option` = might be missing)
    pub ready_blend: Option<PathBuf>, // "Ready"
    pub reference: Option<PathBuf>,  // reference image
    pub spp: Option<PathBuf>,        // "Texture" (Substance Painter project)
    pub txt: Option<PathBuf>,        // description file; its NAME is the UGC name
    pub previews: Vec<PathBuf>,      // `Vec` = growable list
    pub textures: Vec<PathBuf>,
    pub title: String,
    pub title_lc: String, // lower-case title, cached so searching doesn't re-lowercase every frame
    pub description: String,
    pub desc_orig: String, // the description as saved on disk (what "Discard changes" restores)
    pub desc_saved: bool, // true if the .txt on disk has real text in it
    pub dirty: bool,      // true if the description was edited but not saved yet
    pub size_bytes: u64,  // total size of the whole folder
    pub size_label: String, // the same, as text like "86.2 MB"
    /// File sizes are read once while scanning, never per frame.
    pub sizes: HashMap<PathBuf, String>,
}

/// How many checklist items count towards the progress bar.
pub const CHECKS: usize = 5;

impl Item {
    /// Reads a folder from disk and works out which file is which.
    pub fn load(id: String, dir: PathBuf) -> Item {
        // `..Default::default()` fills all other fields with their default (None, empty, 0…).
        let mut it = Item { id: id.clone(), dir: dir.clone(), ..Default::default() };

        // Look at every file directly inside the folder and classify it by extension.
        for f in list_files(&dir) {
            let stem = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let ext = f.extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
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
                    p.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref(),
                    Some("mp4" | "mov" | "webm" | "mkv")
                )
            })
            .collect();
        // textures/ folder: every file (up to 3 levels deep)
        it.textures = list_files_recursive(&dir.join("textures"), 3);

        // Title = .txt file name, description = .txt contents.
        if let Some(t) = &it.txt {
            it.title = t.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            it.description = fs::read_to_string(t).unwrap_or_default();
            it.desc_orig = it.description.clone();
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
    pub fn size(&self, p: &Path) -> &str {
        self.sizes.get(p).map(|s| s.as_str()).unwrap_or("")
    }

    /// Which of the 5 checklist items are done.
    pub fn checks(&self) -> [bool; CHECKS] {
        [
            self.reference.is_some(),
            self.raw_blend.is_some(),
            self.spp.is_some(),
            self.ready_blend.is_some(),
            self.desc_saved,
        ]
    }

    /// 0.0 … 1.0
    pub fn progress(&self) -> f32 {
        self.checks().iter().filter(|c| **c).count() as f32 / CHECKS as f32
    }
}
