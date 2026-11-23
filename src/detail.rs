//! The right-hand detail view of one UGC: the two columns of cards, the reference
//! image thumbnail, and the `Actions` record the UI fills in each frame.

use eframe::egui::{self, Align, Layout, RichText, Sense, Stroke, Vec2};
use eframe::egui::load::SizedTexture;
use std::path::PathBuf;
use crate::constants::MAX_DESC;
use crate::model::{Item, Meta};
use crate::theme::{ACCENT, BAD, CARD_HOVER, ERR, GOOD, MUTED};
use crate::util::fname;
use crate::widgets::{card, card_link, empty_hint, progress_bar, trunc_label, validity_outline};

/// The reference image, shrunk down so it doesn't waste memory.
pub struct Thumb {
    pub path: PathBuf,
    pub tex: Option<egui::TextureHandle>, // the GPU texture, once decoded
    pub done: bool,                       // decoding finished (successfully or not)
}

/// What the user did during one frame (filled in by the UI code, handled afterwards).
#[derive(Default)]
pub struct Actions {
    pub to_open: Option<PathBuf>, // open this file with its default app
    pub open_dir: Option<PathBuf>, // open this folder in Explorer
    pub save_desc: bool,
    pub status_changed: bool,
    pub notes_edited: bool,
    pub rename: Option<String>,
    pub ask_uploaded: Option<String>, // open the "Set as Uploaded" pop-up, prefilled with this link
    pub open_url: Option<String>,     // open this link in the browser
}

/// Left column of the detail view: Reference, Description, Notes.
pub fn left_col(ui: &mut egui::Ui, item: &mut Item, meta: &mut Meta, act: &mut Actions, thumb: Option<&Thumb>) {
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
            // The text box normally draws its own red focus ring (our theme's selection stroke).
            // Turn that off inside this scope so only our green/red validity outline shows.
            let r = ui
                .scope(|ui| {
                    ui.visuals_mut().selection.stroke = Stroke::NONE;
                    ui.add(
                        egui::TextEdit::multiline(&mut item.description)
                            .desired_width(f32::INFINITY)
                            .desired_rows(7),
                    )
                })
                .inner;
            if r.changed() {
                item.dirty = true;
            }
            let count = item.description.chars().count();
            let over = count > MAX_DESC;
            // Outline: red when too long, green while editing a valid text (hidden otherwise).
            if over || item.dirty || r.has_focus() {
                validity_outline(ui, r.rect, !over);
            }
            ui.horizontal_wrapped(|ui| {
                let can_save = item.dirty && !over;
                let b = egui::Button::new("Save to .txt").fill(if can_save { ACCENT } else { CARD_HOVER });
                if ui.add_enabled(can_save, b).clicked() {
                    act.save_desc = true;
                }
                // Throw the edits away and go back to the text saved on disk.
                if item.dirty && ui.button("Discard changes").clicked() {
                    item.description = item.desc_orig.clone();
                    item.dirty = false;
                }
                let col = if over { ERR } else if item.dirty { GOOD } else { MUTED };
                ui.label(RichText::new(format!("{count}/{MAX_DESC}")).size(12.0).color(col));
                if over {
                    ui.label(RichText::new("Too long: shorten it to save").size(12.0).color(ERR));
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
pub fn right_col(ui: &mut egui::Ui, item: &Item, act: &mut Actions) {
    // ── Checklist ──
    card(ui, "Checklist", |ui| {
        // (label, file path if any, is it done?)
        let rows: [(&str, Option<&PathBuf>, bool); 5] = [
            ("Reference image", item.reference.as_ref(), item.reference.is_some()),
            ("Model", item.raw_blend.as_ref(), item.raw_blend.is_some()),
            ("Texture", item.spp.as_ref(), item.spp.is_some()),
            ("Ready", item.ready_blend.as_ref(), item.ready_blend.is_some()),
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
