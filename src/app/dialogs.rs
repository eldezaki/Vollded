//! Pop-ups: About, "Set as Uploaded" (needs a link) and "Delete this UGC?".

use eframe::egui::{self, Align2, RichText, Stroke};
use super::App;
use crate::constants::{COMPONENTS, GITHUB_URL};
use crate::model::Status;
use crate::ops::to_recycle_bin;
use crate::theme::{ACCENT, ERR, GOOD, MUTED};
use crate::util::valid_link;
use crate::widgets::link_text;

impl App {
    // ── About pop-up ──
    pub fn about_ui(&mut self, ctx: &egui::Context) {
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
    pub fn upload_dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(d) = self.upload_dialog.as_mut() else { return };
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
                ui.painter().rect_stroke(r.rect.expand(2.0), 6.0, Stroke::new(2.0, if valid { GOOD } else { ERR }));
                if d.focus {
                    r.request_focus();
                    d.focus = false;
                }
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && valid;
                let (hint, col) = if valid {
                    ("Link looks good.", GOOD)
                } else {
                    ("Enter a full link starting with https://", ERR)
                };
                ui.label(RichText::new(hint).size(12.0).color(col));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(valid, egui::Button::new("Set as Uploaded").fill(ACCENT)).clicked() || enter {
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
    pub fn delete_confirm_ui(&mut self, ctx: &egui::Context) {
        let Some(id) = self.pending_delete.clone() else { return };
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
                ui.label(format!("The whole folder ({}) will be moved to the Recycle Bin.", item.size_label));
                ui.label(RichText::new("Nothing is deleted permanently, it can be restored from the Recycle Bin.").color(MUTED));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new("Move to Recycle Bin").fill(ACCENT)).clicked() {
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
    pub fn delete_ugc(&mut self, id: &str) {
        self.pending_delete = None;
        let Some(pos) = self.items.iter().position(|i| i.id == id) else { return };
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
}
