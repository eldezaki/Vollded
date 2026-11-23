//! Drag & drop: what a drop on the list (new UGC) or on the UGC view (add files) means,
//! plus the "Replace existing files?" confirmation.

use eframe::egui::{self, Align2, RichText};
use std::path::PathBuf;
use super::App;
use crate::ops::{Op, Pending, apply_ops, plan_ops, rejected_note};
use crate::platform::drop_pos;
use crate::theme::{ACCENT, MUTED};
use crate::util::fname;

impl App {
    // ── 6b. Drag & drop: deciding what a drop means ──

    /// Called once with the files that were just dropped on the window.
    pub fn handle_drop(&mut self, ctx: &egui::Context, files: Vec<PathBuf>) {
        let Some(root) = self.root.clone() else {
            self.msg = "Pick your Vollded folder first.".into();
            return;
        };
        let pos = drop_pos(ctx);
        let on_list = pos.map_or(false, |p| self.list_rect.contains(p));
        let on_main = pos.map_or(false, |p| self.main_rect.contains(p));

        if on_list {
            // ── Dropped on the LEFT list → create a brand-new UGC folder ──
            let next = self.items.iter().filter_map(|i| i.id.parse::<u64>().ok()).max().unwrap_or(0) + 1;
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
            self.msg = "Drop onto the UGC list (new UGC) or the UGC view (add to the selected UGC).".into();
        }
    }

    /// Performs the copy, rescans, and tells the user what happened.
    pub fn apply_and_report(&mut self, id: String, ops: Vec<Op>, rejected: Vec<String>, is_new: bool) {
        match apply_ops(&ops) {
            Ok(_) => {
                self.reload(Some(id.clone()));
                let what = if is_new { format!("Created UGC {id}") } else { format!("Updated UGC {id}") };
                self.msg = format!("{what} with {} file(s).{}", ops.len(), rejected_note(&rejected));
            }
            Err(e) => {
                self.reload(Some(id)); // show whatever did get copied
                self.msg = e;
            }
        }
    }

    /// The "Replace existing files?" pop-up.
    pub fn confirm_ui(&mut self, ctx: &egui::Context) {
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
                            RichText::new(format!("• {}: replace \"{}\" with \"{}\"", op.label, old, fname(&op.src)))
                                .color(MUTED),
                        );
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new("Replaced files are moved to the Recycle Bin, not deleted.").color(MUTED));
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
}
