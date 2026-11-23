//! File operations that change things on disk: renaming a UGC, planning and copying
//! dropped files, renumbering folders after a reorder, and the Recycle Bin.
//! Nothing in this app is ever deleted permanently.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use crate::constants::MAX_NAME;
use crate::model::Item;
use crate::util::{fname, same_path};

/// Renames a UGC = renames its .txt file (or creates one). Returns `Err(message)` if invalid.
pub fn do_rename(item: &mut Item, new: &str) -> Result<(), String> {
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

/// One file operation: copy `src` to `dest`, then delete the `remove` file (if any).
pub struct Op {
    pub src: PathBuf,
    pub dest: PathBuf,
    pub remove: Option<PathBuf>, // an older file this one replaces under a different name
    pub label: &'static str,     // "Model", "Texture", …
    pub existing: Option<String>, // Some(name) if something already exists → needs confirmation
}

/// A drop that is waiting for the user to click "Replace" or "Cancel".
pub struct Pending {
    pub id: String,
    pub ops: Vec<Op>,
    pub rejected: Vec<String>,
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
pub fn plan_ops(id: &str, dir: &Path, item: Option<&Item>, files: &[PathBuf]) -> (Vec<Op>, Vec<String>) {
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
        let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();

        // A tuple of: (slot key, label, destination path, existing file in that slot).
        let (slot, label, dest, old): (Option<&'static str>, &'static str, PathBuf, Option<PathBuf>) =
            match ext.as_str() {
                "blend" => (Some("model"), "Model", dir.join(format!("{id}.blend")), item.and_then(|i| i.raw_blend.clone())),
                "spp" => (Some("texture"), "Texture", dir.join(format!("{id}.spp")), item.and_then(|i| i.spp.clone())),
                "jpg" | "jpeg" => (Some("reference"), "Reference image", dir.join(format!("{id}.{ext}")), item.and_then(|i| i.reference.clone())),
                "txt" => (Some("description"), "Description", dir.join(&name), item.and_then(|i| i.txt.clone())),
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
        let op = Op { src: src.clone(), dest, remove, label, existing };

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
pub fn to_recycle_bin(p: &Path) -> Result<(), String> {
    trash::delete(p).map_err(|e| format!("Could not move {} to the Recycle Bin: {e}", fname(p)))
}

/// Copies the files. We COPY (not move) so the originals stay where they are.
/// A file that gets replaced is moved to the Recycle Bin first (a plain copy would overwrite
/// it for good).
pub fn apply_ops(ops: &[Op]) -> Result<(), String> {
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
            format!("Copy failed for {}: {e}. A replaced file can be restored from the Recycle Bin.", fname(&op.src))
        })?;
        // An older file stored under a different name is replaced too → Recycle Bin.
        if let Some(old) = &op.remove {
            to_recycle_bin(old)?;
        }
    }
    Ok(())
}

/// " Rejected (unsupported): a.zip, b.exe." or "" if nothing was rejected.
pub fn rejected_note(r: &[String]) -> String {
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
pub fn renumber_folders(root: &Path, plan: &[(String, String)]) -> Result<Vec<String>, String> {
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
pub fn rename_inner_files(dir: &Path, old: &str, new: &str, problems: &mut Vec<String>) {
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
