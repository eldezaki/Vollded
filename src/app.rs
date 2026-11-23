//! The `App` struct (all program state), start-up, loading/reloading the folder, and
//! `update()`, which egui calls every frame.
//!
//! The rest of `impl App` lives in the sub-modules next to this file:
//!   drops    - drag & drop of files onto the window
//!   dialogs  - About / Set as Uploaded / Delete pop-ups
//!   reorder  - dragging cards to reorder (renames the folders)
//!   panels   - the top bar, the left list and the detail view

mod dialogs;
mod drops;
mod panels;
mod reorder;

use eframe::egui::{self, Align, Layout, Rect, RichText, Vec2};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
use crate::config::{exe_dir, load_cfg_root, save_cfg_root};
use crate::constants::GITHUB_URL;
use crate::detail::Thumb;
use crate::model::{Item, Meta, Status};
use crate::ops::Pending;
use crate::theme::{BG, MUTED, PANEL, setup_style};
use crate::util::{fmt_bytes, load_meta, meta_path, scan};
use crate::widgets::{draw_drop_zone, github_button, info_button, link_text, trunc_label};

/// Message sent from the decoder thread back to the UI thread.
type ThumbMsg = (PathBuf, Option<egui::ColorImage>);

/// State of the "Set as Uploaded" pop-up.
struct UploadDialog {
    id: String,    // which UGC
    link: String,  // text typed so far
    focus: bool,   // put the cursor in the box when it first opens
}

/// All the state of the running program lives in this one struct.
pub struct App {
    root_input: String, // text in the path box
    root: Option<PathBuf>,
    items: Vec<Item>,
    meta: HashMap<String, Meta>, // folder id → status/notes
    selected: Option<usize>,     // index into `items`
    search: String,
    filter: Option<Status>, // None = show all statuses
    msg: String,            // message in the bottom bar
    rename_buf: Option<String>, // Some(text) while the rename box is open
    rename_focus: bool,
    // Cached results: recalculated only when something changes (saves CPU).
    visible: Vec<usize>, // indices of items that pass search + filter
    view_dirty: bool,
    last_search: String,
    last_filter: Option<Status>,
    stats: (usize, usize, usize), // (total, complete, uploaded)
    total_bytes: u64,             // size of the whole Vollded folder
    meta_save_at: Option<Instant>, // when notes were last edited (for delayed saving)
    thumb: Option<Thumb>,
    thumb_tx: Sender<ThumbMsg>,
    thumb_rx: Receiver<ThumbMsg>,
    // Drag & drop
    list_rect: Rect, // where the left list was drawn last frame
    main_rect: Rect, // where the UGC view was drawn last frame
    pending: Option<Pending>, // drop waiting for confirmation
    pending_delete: Option<String>, // id of the UGC waiting for "move to Recycle Bin?" confirmation
    upload_dialog: Option<UploadDialog>, // the "paste the published link" pop-up
    show_about: bool,                    // is the About pop-up open?
    drag_from: Option<usize>, // index of the card being dragged to reorder
    last_frame: Instant,      // for the frame-rate limiter
    side_min_w: f32,          // narrowest the left panel may get (measured on the first frame)
}

// `impl App { … }` = the methods (functions) that belong to App.
impl App {
    /// Runs once when the program starts.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
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
    pub fn load_root(&mut self, root: PathBuf) -> bool {
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
    pub fn open_root(&mut self, root: PathBuf) {
        if self.load_root(root.clone()) {
            save_cfg_root(&root);
        }
    }

    /// Rescans the disk. `select` = the UGC id to select afterwards (else keep the current one).
    pub fn reload(&mut self, select: Option<String>) {
        let Some(root) = self.root.clone() else { return };
        let old_idx = self.selected;
        let keep = select.or_else(|| self.selected.and_then(|i| self.items.get(i)).map(|i| i.id.clone()));
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
    pub fn save_meta(&mut self) {
        if let Some(root) = &self.root {
            if let Ok(s) = serde_json::to_string_pretty(&self.meta) {
                if fs::write(meta_path(root), s).is_err() {
                    self.msg = "Could not save metadata file.".into();
                }
            }
        }
        self.meta_save_at = None;
    }

    pub fn status_of(&self, id: &str) -> Status {
        self.meta.get(id).map(|m| m.status).unwrap_or_default()
    }

    /// Recomputes the filtered list and the totals. Only called when something changed.
    pub fn rebuild_view(&mut self) {
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
        let up = self.items.iter().filter(|i| self.status_of(&i.id) == Status::Uploaded).count();
        self.stats = (total, done, up);
        self.total_bytes = self.items.iter().map(|i| i.size_bytes).sum();
        self.view_dirty = false;
    }

    /// Starts decoding the reference image on a background thread (if not already loaded).
    pub fn ensure_thumb(&mut self, ctx: &egui::Context, want: Option<PathBuf>) {
        let Some(p) = want else {
            self.thumb = None;
            return;
        };
        if self.thumb.as_ref().map_or(false, |t| t.path == p) {
            return; // already loading / loaded
        }
        self.thumb = Some(Thumb { path: p.clone(), tex: None, done: false });
        let tx = self.thumb_tx.clone();
        let ctx = ctx.clone();
        // `move` gives the closure ownership of the variables it uses (needed for threads).
        std::thread::spawn(move || {
            let img = image::open(&p).ok().map(|im| {
                let t = im.thumbnail(720, 720).to_rgba8(); // shrink to max 720px
                egui::ColorImage::from_rgba_unmultiplied([t.width() as usize, t.height() as usize], t.as_raw())
            });
            let _ = tx.send((p, img)); // hand the result to the UI thread
            ctx.request_repaint(); // wake the UI up so it shows the image
        });
    }
}

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
        let dropped: Vec<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if !dropped.is_empty() && self.pending.is_none() && self.pending_delete.is_none() && self.upload_dialog.is_none() {
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
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::symmetric(14.0, 10.0)))
            .show(ctx, |ui| self.top_bar(ui));

        // Bottom bar: message on the left; folder size + GitHub link on the right.
        egui::TopBottomPanel::bottom("status")
            .exact_height(28.0)
            .frame(egui::Frame::none().fill(BG).inner_margin(egui::Margin::symmetric(16.0, 4.0)))
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
                        if link_text(ui, &txt, 12.0).on_hover_text("Open the Vollded folder").clicked() {
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
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop_overlay")));
            draw_drop_zone(&p, self.list_rect, "Drop here\nto create a new UGC");
            draw_drop_zone(&p, self.main_rect, "Drop here to add to this UGC\n(.blend .spp .jpg .txt .mp4 .png)");
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
