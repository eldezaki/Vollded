#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui::{
    self, load::SizedTexture, Align, Align2, Color32, FontId, Layout, RichText, Sense, Stroke, Vec2,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

const MAX_NAME: usize = 50;

// ───────────────────────── Theme ─────────────────────────
const BG: Color32 = Color32::from_rgb(15, 15, 18);
const PANEL: Color32 = Color32::from_rgb(21, 21, 26);
const CARD: Color32 = Color32::from_rgb(31, 31, 38);
const CARD_HOVER: Color32 = Color32::from_rgb(42, 42, 52);
const ACCENT: Color32 = Color32::from_rgb(225, 48, 60);
const GOOD: Color32 = Color32::from_rgb(52, 211, 153);
const WARN: Color32 = Color32::from_rgb(251, 191, 36);
const BAD: Color32 = Color32::from_rgb(120, 126, 148);
const TEXT: Color32 = Color32::from_rgb(232, 234, 242);
const MUTED: Color32 = Color32::from_rgb(138, 145, 168);

fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = CARD;
    v.extreme_bg_color = BG;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
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

    let mut s = (*ctx.style()).clone();
    s.spacing.item_spacing = Vec2::new(10.0, 10.0);
    s.spacing.button_padding = Vec2::new(12.0, 6.0);
    // Less work per frame: no hover/transition animations (each one forces extra repaints),
    // and no per-label text-selection tracking (every label would otherwise be interactive).
    s.animation_time = 0.0;
    s.interaction.selectable_labels = false;
    ctx.set_style(s);
}

// ───────────────────────── Portable config (next to the exe) ─────────────────────────
#[derive(Default, Serialize, Deserialize)]
struct Config {
    root: String,
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn cfg_path() -> PathBuf {
    exe_dir().join("vollded_ugc.json")
}

fn load_cfg_root() -> Option<PathBuf> {
    let cfg: Config = serde_json::from_str(&fs::read_to_string(cfg_path()).ok()?).ok()?;
    if cfg.root.is_empty() {
        return None;
    }
    let p = PathBuf::from(&cfg.root);
    Some(if p.is_relative() {
        exe_dir().join(p)
    } else {
        p
    })
}

fn save_cfg_root(root: &Path) {
    // Store relative to the exe when possible so the whole folder can be moved around.
    let stored = root
        .strip_prefix(exe_dir())
        .map(|r| r.to_path_buf())
        .unwrap_or_else(|_| root.to_path_buf());
    let cfg = Config {
        root: stored.display().to_string(),
    };
    if let Ok(s) = serde_json::to_string_pretty(&cfg) {
        let _ = fs::write(cfg_path(), s); // read-only location? just skip silently
    }
}

// ───────────────────────── Data ─────────────────────────
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Debug)]
enum Status {
    #[default]
    Todo,
    Modeling,
    Texturing,
    Ready,
    Uploaded,
}

impl Status {
    const ALL: [Status; 5] = [
        Status::Todo,
        Status::Modeling,
        Status::Texturing,
        Status::Ready,
        Status::Uploaded,
    ];
    fn label(self) -> &'static str {
        match self {
            Status::Todo => "To do",
            Status::Modeling => "Modeling",
            Status::Texturing => "Texturing",
            Status::Ready => "Ready",
            Status::Uploaded => "Uploaded",
        }
    }
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

#[derive(Clone, Default, Serialize, Deserialize)]
struct Meta {
    status: Status,
    notes: String,
}

#[derive(Default)]
struct Item {
    id: String,
    dir: PathBuf,
    raw_blend: Option<PathBuf>,
    ready_blend: Option<PathBuf>,
    reference: Option<PathBuf>,
    spp: Option<PathBuf>,
    txt: Option<PathBuf>,
    previews: Vec<PathBuf>,
    textures: Vec<PathBuf>,
    title: String,
    title_lc: String,
    description: String,
    desc_saved: bool,
    dirty: bool,
    /// File sizes are read once at scan time, never per frame.
    sizes: HashMap<PathBuf, String>,
}

const CHECKS: usize = 5;

impl Item {
    fn load(id: String, dir: PathBuf) -> Item {
        let mut it = Item {
            id: id.clone(),
            dir: dir.clone(),
            ..Default::default()
        };

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
                        it.ready_blend = Some(f);
                    } else if stem == id || it.raw_blend.is_none() {
                        it.raw_blend = Some(f);
                    }
                }
                "jpg" | "jpeg" | "png" => {
                    if it.reference.is_none() || stem == id {
                        it.reference = Some(f);
                    }
                }
                "spp" => {
                    if !lstem.contains("autosave") && (it.spp.is_none() || stem == id) {
                        it.spp = Some(f);
                    }
                }
                "txt" => it.txt = Some(f),
                _ => {}
            }
        }

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
        it.textures = list_files_recursive(&dir.join("textures"), 3);

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

        let known: Vec<PathBuf> = [&it.raw_blend, &it.ready_blend, &it.reference, &it.spp]
            .into_iter()
            .flatten()
            .cloned()
            .chain(it.previews.iter().cloned())
            .collect();
        for p in known {
            let s = human_size(&p);
            it.sizes.insert(p, s);
        }
        it
    }

    fn size(&self, p: &Path) -> &str {
        self.sizes.get(p).map(|s| s.as_str()).unwrap_or("")
    }

    fn checks(&self) -> [bool; CHECKS] {
        [
            self.reference.is_some(),
            self.raw_blend.is_some(),
            self.spp.is_some(),
            self.ready_blend.is_some(),
            self.desc_saved,
        ]
    }

    fn progress(&self) -> f32 {
        self.checks().iter().filter(|c| **c).count() as f32 / CHECKS as f32
    }
}

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

fn list_files_recursive(dir: &Path, depth: u8) -> Vec<PathBuf> {
    let mut out = vec![];
    if depth == 0 {
        return out;
    }
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(list_files_recursive(&p, depth - 1));
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

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
    items.sort_by(|a, b| match (a.id.parse::<u64>(), b.id.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        (Ok(_), Err(_)) => std::cmp::Ordering::Less,
        (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
        _ => a.id.cmp(&b.id),
    });
    items
}

fn meta_path(root: &Path) -> PathBuf {
    root.join(".ugc_manager.json")
}

fn load_meta(root: &Path) -> HashMap<String, Meta> {
    fs::read_to_string(meta_path(root))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn human_size(p: &Path) -> String {
    let n = fs::metadata(p).map(|m| m.len()).unwrap_or(0) as f64;
    if n > 1e9 {
        format!("{:.2} GB", n / 1e9)
    } else if n > 1e6 {
        format!("{:.1} MB", n / 1e6)
    } else {
        format!("{:.0} KB", n / 1e3)
    }
}

fn fname(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn do_rename(item: &mut Item, new: &str) -> Result<(), String> {
    let new = new.trim();
    if new.is_empty() {
        return Err("Name can't be empty.".into());
    }
    if new.chars().count() > MAX_NAME {
        return Err(format!("Name is too long (max {MAX_NAME} characters)."));
    }
    if new.chars().any(|c| "\\/:*?\"<>|".contains(c)) || new.ends_with('.') {
        return Err("Name can't contain \\ / : * ? \" < > | or end with a dot.".into());
    }
    let target = item.dir.join(format!("{new}.txt"));
    let same = item
        .txt
        .as_ref()
        .map_or(false, |o| fname(o).eq_ignore_ascii_case(&fname(&target)));
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

// ───────────────────────── Thumbnail (decoded off-thread, downscaled) ─────────────────────────
struct Thumb {
    path: PathBuf,
    tex: Option<egui::TextureHandle>,
    done: bool,
}
type ThumbMsg = (PathBuf, Option<egui::ColorImage>);

// ───────────────────────── App ─────────────────────────
struct App {
    root_input: String,
    root: Option<PathBuf>,
    items: Vec<Item>,
    meta: HashMap<String, Meta>,
    selected: Option<usize>,
    search: String,
    filter: Option<Status>,
    msg: String,
    rename_buf: Option<String>,
    rename_focus: bool,
    // cached view state: recomputed only when something changes, not every frame
    visible: Vec<usize>,
    view_dirty: bool,
    last_search: String,
    last_filter: Option<Status>,
    stats: (usize, usize, usize),
    meta_save_at: Option<Instant>,
    thumb: Option<Thumb>,
    thumb_tx: Sender<ThumbMsg>,
    thumb_rx: Receiver<ThumbMsg>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_style(&cc.egui_ctx);
        let (tx, rx) = channel();
        let mut app = App {
            root_input: String::new(),
            root: None,
            items: vec![],
            meta: HashMap::new(),
            selected: None,
            search: String::new(),
            filter: None,
            msg: "Pick your Vollded folder to get started.".into(),
            rename_buf: None,
            rename_focus: false,
            visible: vec![],
            view_dirty: true,
            last_search: String::new(),
            last_filter: None,
            stats: (0, 0, 0),
            meta_save_at: None,
            thumb: None,
            thumb_tx: tx,
            thumb_rx: rx,
        };
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
        self.view_dirty = true;
        true
    }

    fn open_root(&mut self, root: PathBuf) {
        if self.load_root(root.clone()) {
            save_cfg_root(&root);
        }
    }

    fn refresh(&mut self) {
        if let Some(root) = self.root.clone() {
            let keep = self
                .selected
                .and_then(|i| self.items.get(i))
                .map(|i| i.id.clone());
            self.items = scan(&root);
            self.selected = keep.and_then(|k| self.items.iter().position(|i| i.id == k));
            self.thumb = None;
            self.view_dirty = true;
            self.msg = "Rescanned folders.".into();
        }
    }

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
        self.view_dirty = false;
    }

    fn ensure_thumb(&mut self, ctx: &egui::Context, want: Option<PathBuf>) {
        let Some(p) = want else {
            self.thumb = None;
            return;
        };
        if self.thumb.as_ref().map_or(false, |t| t.path == p) {
            return;
        }
        self.thumb = Some(Thumb {
            path: p.clone(),
            tex: None,
            done: false,
        });
        let tx = self.thumb_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let img = image::open(&p).ok().map(|im| {
                let t = im.thumbnail(720, 720).to_rgba8();
                egui::ColorImage::from_rgba_unmultiplied(
                    [t.width() as usize, t.height() as usize],
                    t.as_raw(),
                )
            });
            let _ = tx.send((p, img));
            ctx.request_repaint();
        });
    }

    // ── top bar ──
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let (total, done, up) = self.stats;
        let w = ui.available_width();
        let show_stats = w > 1000.0;
        let reserve = 140.0 + 210.0 + 40.0 + if show_stats { 330.0 } else { 0.0 };
        let path_w = (w - reserve).clamp(100.0, 480.0);

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Vollded UGC")
                    .size(20.0)
                    .strong()
                    .color(ACCENT),
            );
            ui.add_space(8.0);
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.root_input)
                    .hint_text("Path to Vollded folder…")
                    .desired_width(path_w),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.open_root(PathBuf::from(self.root_input.trim()));
            }
            if ui.button("Browse").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.open_root(p);
                }
            }
            if ui.button("Rescan").clicked() {
                self.refresh();
            }
            if show_stats {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(format!("{up} uploaded")).color(ACCENT));
                    ui.label(RichText::new("·").color(MUTED));
                    ui.label(RichText::new(format!("{done}/{total} complete")).color(GOOD));
                    ui.label(RichText::new("·").color(MUTED));
                    ui.label(RichText::new(format!("{total} items")).color(MUTED));
                });
            }
        });
    }

    // ── left list (virtualized: only visible rows are drawn) ──
    fn list_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search…")
                .desired_width(f32::INFINITY),
        );
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
            if ui.selectable_label(self.filter.is_none(), "All").clicked() {
                self.filter = None;
            }
            for s in Status::ALL {
                if ui
                    .selectable_label(self.filter == Some(s), s.label())
                    .clicked()
                {
                    self.filter = if self.filter == Some(s) {
                        None
                    } else {
                        Some(s)
                    };
                }
            }
        });

        let n = self.visible.len();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, 70.0, n, |ui, range| {
                for row in range {
                    let i = self.visible[row];
                    let (rect, resp) = ui
                        .allocate_exact_size(Vec2::new(ui.available_width(), 70.0), Sense::click());
                    let it = &self.items[i];
                    let st = self.status_of(&it.id);
                    let prog = it.progress();
                    let sel = self.selected == Some(i);
                    let fill = if sel {
                        ACCENT.gamma_multiply(0.20)
                    } else if resp.hovered() {
                        CARD_HOVER
                    } else {
                        CARD
                    };
                    let p = ui.painter_at(rect);
                    p.rect_filled(rect, 10.0, fill);
                    if sel {
                        p.rect_stroke(rect, 10.0, Stroke::new(1.5, ACCENT));
                    }
                    let badge = egui::Rect::from_min_size(
                        rect.min + Vec2::new(10.0, 12.0),
                        Vec2::splat(46.0),
                    );
                    p.rect_filled(badge, 10.0, BG);
                    p.text(
                        badge.center(),
                        Align2::CENTER_CENTER,
                        &it.id,
                        FontId::proportional(18.0),
                        ACCENT,
                    );

                    let tx = rect.min.x + 68.0;
                    let tw = (rect.max.x - tx - 10.0).max(10.0);
                    ellipsized(
                        &p,
                        egui::pos2(tx, rect.min.y + 10.0),
                        &it.title,
                        FontId::proportional(14.5),
                        TEXT,
                        tw,
                    );
                    p.circle_filled(egui::pos2(tx + 4.0, rect.min.y + 39.0), 4.0, st.color());
                    p.text(
                        egui::pos2(tx + 14.0, rect.min.y + 32.0),
                        Align2::LEFT_TOP,
                        st.label(),
                        FontId::proportional(12.0),
                        MUTED,
                    );
                    let bar = egui::Rect::from_min_size(
                        egui::pos2(tx, rect.min.y + 55.0),
                        Vec2::new(tw, 4.0),
                    );
                    p.rect_filled(bar, 2.0, BG);
                    let col = if prog >= 1.0 { GOOD } else { WARN };
                    p.rect_filled(
                        egui::Rect::from_min_size(bar.min, Vec2::new(bar.width() * prog, 4.0)),
                        2.0,
                        col,
                    );
                    if resp.clicked() {
                        self.selected = Some(i);
                        self.rename_buf = None;
                    }
                }
            });
    }

    // ── detail ──
    fn detail_ui(&mut self, ui: &mut egui::Ui, idx: usize) {
        let ctx = ui.ctx().clone();
        let same = match (&self.items[idx].reference, &self.thumb) {
            (Some(p), Some(t)) => &t.path == p,
            (None, None) => true,
            _ => false,
        };
        if !same {
            let want = self.items[idx].reference.clone();
            self.ensure_thumb(&ctx, want);
        }

        let thumb = self.thumb.as_ref();
        let item = &mut self.items[idx];
        let meta = self.meta.entry(item.id.clone()).or_default();
        let rename_buf = &mut self.rename_buf;
        let rename_focus = &mut self.rename_focus;
        let mut act = Actions::default();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // ── header: name + rename ──
                if let Some(buf) = rename_buf.as_mut() {
                    let r = ui.add(
                        egui::TextEdit::singleline(buf)
                            .font(FontId::proportional(22.0))
                            .char_limit(MAX_NAME)
                            .desired_width(f32::INFINITY),
                    );
                    if *rename_focus {
                        r.request_focus();
                        *rename_focus = false;
                    }
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let mut cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    ui.horizontal(|ui| {
                        if ui
                            .add(egui::Button::new("Save name").fill(ACCENT))
                            .clicked()
                            || enter
                        {
                            act.rename = Some(buf.clone());
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                        ui.label(
                            RichText::new(format!("{}/{}", buf.chars().count(), MAX_NAME))
                                .size(12.0)
                                .color(MUTED),
                        );
                    });
                    if cancel {
                        *rename_buf = None;
                    }
                } else {
                    ui.add(
                        egui::Label::new(RichText::new(&item.title).size(26.0).strong())
                            .wrap_mode(egui::TextWrapMode::Wrap),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Rename").clicked() {
                            *rename_buf = Some(item.title.chars().take(MAX_NAME).collect());
                            *rename_focus = true;
                        }
                        if ui.button("Open folder").clicked() {
                            act.to_open = Some(item.dir.clone());
                        }
                    });
                }

                // ── status ──
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
                        if ui.add(b).clicked() && !sel {
                            meta.status = s;
                            act.status_changed = true;
                        }
                    }
                });

                // ── body: two columns when wide, one column when narrow ──
                if ui.available_width() > 820.0 {
                    ui.columns(2, |cols| {
                        left_col(&mut cols[0], item, meta, &mut act, thumb);
                        right_col(&mut cols[1], item, &mut act);
                    });
                } else {
                    right_col(ui, item, &mut act);
                    left_col(ui, item, meta, &mut act, thumb);
                }
            });

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
            self.meta_save_at = Some(Instant::now()); // debounced write, see update()
        }
        if let Some(p) = act.to_open {
            if let Err(e) = open::that(&p) {
                self.msg = format!("Could not open {}: {e}", p.display());
            }
        }
    }
}

// ───────────────────────── UI helpers ─────────────────────────
#[derive(Default)]
struct Actions {
    to_open: Option<PathBuf>,
    save_desc: bool,
    status_changed: bool,
    notes_edited: bool,
    rename: Option<String>,
}

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

fn empty_hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).italics().color(MUTED));
}

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

fn left_col(
    ui: &mut egui::Ui,
    item: &mut Item,
    meta: &mut Meta,
    act: &mut Actions,
    thumb: Option<&Thumb>,
) {
    card(ui, "Reference", |ui| {
        if let Some(p) = &item.reference {
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
                        act.to_open = Some(p.clone());
                    }
                }
                Some((None, true)) => empty_hint(ui, "Preview unavailable"),
                _ => empty_hint(ui, "Loading…"),
            }
            ui.label(RichText::new(fname(p)).size(12.0).color(MUTED));
        } else {
            empty_hint(ui, "No reference image found");
        }
    });

    card(ui, "Description", |ui| {
        if item.txt.is_none() {
            empty_hint(ui, "No .txt yet. Use Rename to create it.");
        } else {
            let r = ui.add(
                egui::TextEdit::multiline(&mut item.description)
                    .desired_width(f32::INFINITY)
                    .desired_rows(7),
            );
            if r.changed() {
                item.dirty = true;
            }
            ui.horizontal_wrapped(|ui| {
                let b = egui::Button::new("Save to .txt").fill(if item.dirty {
                    ACCENT
                } else {
                    CARD_HOVER
                });
                if ui.add_enabled(item.dirty, b).clicked() {
                    act.save_desc = true;
                }
                ui.label(
                    RichText::new(format!("{} chars", item.description.chars().count()))
                        .size(12.0)
                        .color(MUTED),
                );
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

fn right_col(ui: &mut egui::Ui, item: &Item, act: &mut Actions) {
    card(ui, "Checklist", |ui| {
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
        ui.add(
            egui::ProgressBar::new(prog)
                .fill(if prog >= 1.0 { GOOD } else { WARN })
                .text(format!("{:.0}% complete", prog * 100.0)),
        );
    });

    card(ui, &format!("Previews ({})", item.previews.len()), |ui| {
        if item.previews.is_empty() {
            empty_hint(ui, "No videos in previews/");
        }
        for p in &item.previews {
            ui.horizontal(|ui| {
                if ui.button("▶").clicked() {
                    act.to_open = Some(p.clone());
                }
                let name_w = (ui.available_width() - 75.0).max(40.0);
                trunc_label(ui, &fname(p), name_w);
                ui.label(RichText::new(item.size(p)).size(12.0).color(MUTED));
            });
        }
    });

    card(ui, &format!("Textures ({})", item.textures.len()), |ui| {
        if item.textures.is_empty() {
            empty_hint(ui, "No files in textures/");
        } else {
            if ui.button("Open textures folder").clicked() {
                act.to_open = Some(item.dir.join("textures"));
            }
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .id_salt("tex")
                .show(ui, |ui| {
                    for p in &item.textures {
                        ui.add(
                            egui::Label::new(RichText::new(fname(p)).size(12.5).color(MUTED))
                                .wrap_mode(egui::TextWrapMode::Truncate),
                        );
                    }
                });
        }
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // finished thumbnail decodes
        while let Ok((path, img)) = self.thumb_rx.try_recv() {
            if let Some(t) = self.thumb.as_mut() {
                if t.path == path {
                    t.done = true;
                    t.tex = img.map(|i| ctx.load_texture("ref", i, egui::TextureOptions::LINEAR));
                }
            }
        }

        // cached list/stats: rebuilt only when inputs change
        if self.search != self.last_search || self.filter != self.last_filter {
            self.last_search = self.search.clone();
            self.last_filter = self.filter;
            self.view_dirty = true;
        }
        if self.view_dirty {
            self.rebuild_view();
        }

        egui::TopBottomPanel::top("top")
            .frame(
                egui::Frame::none()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(14.0, 10.0)),
            )
            .show(ctx, |ui| self.top_bar(ui));

        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .frame(
                egui::Frame::none()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(16.0, 4.0)),
            )
            .show(ctx, |ui| {
                ui.label(RichText::new(&self.msg).size(12.0).color(MUTED));
            });

        let side_w = if ctx.screen_rect().width() < 800.0 {
            220.0
        } else {
            320.0
        };
        egui::SidePanel::left("list")
            .default_width(side_w)
            .width_range(190.0..=440.0)
            .frame(egui::Frame::none().fill(PANEL).inner_margin(10.0))
            .show(ctx, |ui| self.list_ui(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(16.0))
            .show(ctx, |ui| match self.selected {
                Some(i) if i < self.items.len() => self.detail_ui(ui, i),
                _ => {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Select a UGC item, or pick your Vollded folder above")
                                .size(16.0)
                                .color(MUTED),
                        );
                    });
                }
            });

        // debounced notes save (also flushed when the window is closing)
        if let Some(t) = self.meta_save_at {
            let wait = Duration::from_millis(900);
            if ctx.input(|i| i.viewport().close_requested()) || t.elapsed() >= wait {
                self.save_meta();
            } else {
                ctx.request_repaint_after(wait - t.elapsed());
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([980.0, 600.0])
            .with_title("Vollded UGC"),
        ..Default::default()
    };
    eframe::run_native(
        "Vollded UGC",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
