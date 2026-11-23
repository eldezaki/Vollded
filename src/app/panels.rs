//! The three panels: top bar, left list of UGC cards, and the detail view of the selected UGC.

use eframe::egui::{self, Align, Align2, Color32, CursorIcon, FontId, Layout, Rect, RichText, Sense, Stroke, Vec2};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use super::{App, UploadDialog};
use crate::constants::MAX_NAME;
use crate::detail::{Actions, left_col, right_col};
use crate::model::Status;
use crate::ops::do_rename;
use crate::theme::{ACCENT, BG, CARD, CARD_HOVER, ERR, GOOD, MUTED, TEXT, WARN};
use crate::widgets::{draw_trash, ellipsized};

impl App {
    /// Width the left panel needs so the size label ("12.3 MB") never gets cut off or overlaps.
    pub fn measure_side_min(ctx: &egui::Context) -> f32 {
        let width = |t: &str| {
            ctx.fonts(|f| f.layout_no_wrap(t.to_owned(), FontId::proportional(12.0), Color32::WHITE).size().x)
        };
        let status_w = Status::ALL.iter().map(|s| width(s.label())).fold(0.0_f32, f32::max);
        let size_w = width("888.88 GB"); // the widest size text that can appear
        // badge+title offset 68 · dot indent 14 · status · gap 12 · size · right padding 10
        // + panel margins 20 + scroll bar 16
        68.0 + 14.0 + status_w + 12.0 + size_w + 10.0 + 20.0 + 16.0
    }

    // ── Top bar ──
    // Everything is laid out RIGHT-TO-LEFT in one row: stats (far right), Rescan, Browse, then the
    // path box takes whatever width is left, and the title sits at the far left. That way the
    // buttons and stats stay in the same place at every window width, even the smallest.
    pub fn top_bar(&mut self, ui: &mut egui::Ui) {
        let (total, done, up) = self.stats;
        // Exact width of the title, measured with the real font.
        let title_w = ui.fonts(|f| {
            f.layout_no_wrap("Vollded UGC".to_owned(), FontId::proportional(20.0), Color32::WHITE).size().x
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
                ui.label(RichText::new("Vollded UGC").size(20.0).strong().color(ACCENT));
            });
        });
    }

    // ── Left list (search, status dropdown, cards) ──
    pub fn list_ui(&mut self, ui: &mut egui::Ui) {
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
                // No outline on the selected card: its tinted background is enough.

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
    pub fn detail_ui(&mut self, ui: &mut egui::Ui, idx: usize) {
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

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
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
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && valid;
                let mut cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
                ui.horizontal(|ui| {
                    if ui.add_enabled(valid, egui::Button::new("Save name").fill(ACCENT)).clicked() || enter {
                        act.rename = Some(buf.clone());
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    let counter_color = if count > MAX_NAME { ERR } else { MUTED };
                    ui.label(RichText::new(format!("{count}/{MAX_NAME}")).size(12.0).color(counter_color));
                    if count > MAX_NAME {
                        ui.label(RichText::new("Too long: shorten it to save").size(12.0).color(ERR));
                    }
                });
                if cancel {
                    *rename_buf = None;
                }
            } else {
                // Normal mode: show the title and the Rename / Open folder buttons.
                ui.add(egui::Label::new(RichText::new(&item.title).size(26.0).strong()).wrap_mode(egui::TextWrapMode::Wrap));
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
                        if ui.add(egui::Button::new("Open published UGC").fill(ACCENT.gamma_multiply(0.6))).clicked() {
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
                    let txt_col = if !sel { TEXT } else if s == Status::Uploaded { Color32::WHITE } else { BG };
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
                        item.desc_orig = item.description.clone();
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
            self.upload_dialog = Some(UploadDialog { id: item_id, link, focus: true });
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
