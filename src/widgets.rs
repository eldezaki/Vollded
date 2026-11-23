//! Small reusable UI pieces: cards, link text, progress bar, icon buttons, drop zones…

use eframe::egui::{self, Align, Align2, Color32, CursorIcon, FontId, Layout, Rect, RichText, Sense, Stroke, Vec2};
use crate::theme::{ACCENT, BG, CARD, ERR, GOOD, MUTED, TEXT};

/// A rounded "card" container with a plain title. `body` is a closure (an inline function)
/// that draws the card's contents.
pub fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none().fill(CARD).rounding(12.0).inner_margin(14.0).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(title).size(15.0).strong());
        ui.add_space(2.0);
        body(ui);
    });
}

/// Like `card`, but the title is clickable and ends with a white ">" arrow. Returns true if clicked.
pub fn card_link(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let mut clicked = false;
    egui::Frame::none().fill(CARD).rounding(12.0).inner_margin(14.0).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        // Lay the title text out first so we know how wide the clickable area must be.
        let galley = ui.painter().layout_no_wrap(title.to_owned(), FontId::proportional(15.0), TEXT);
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
pub fn empty_hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).italics().color(MUTED));
}

/// A label that is cut off with "…" when it doesn't fit in `width`.
pub fn trunc_label(ui: &mut egui::Ui, text: &str, width: f32) {
    ui.allocate_ui_with_layout(Vec2::new(width, 20.0), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_size(Vec2::new(width, 20.0));
        ui.set_max_width(width);
        ui.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Truncate));
    });
}

/// Paints single-line text that is shortened with "…" to fit `max_w` (used on list cards).
pub fn ellipsized(p: &egui::Painter, pos: egui::Pos2, text: &str, font: FontId, color: Color32, max_w: f32) {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap.max_width = max_w;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let g = p.layout_job(job);
    p.galley(pos, g, color);
}

/// Progress bar with always-readable text: white text with a dark outline on a darker fill.
pub fn progress_bar(ui: &mut egui::Ui, prog: f32, text: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 6.0, BG); // empty track
    let fill = if prog >= 1.0 { Color32::from_rgb(14, 140, 95) } else { Color32::from_rgb(170, 110, 6) };
    let w = rect.width() * prog;
    if w > 0.0 {
        p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(w, rect.height())), 6.0, fill);
    }
    let font = FontId::proportional(13.5);
    // Draw the text 4 times in black, shifted 1px each way, to make an outline…
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        p.text(rect.center() + Vec2::new(dx, dy), Align2::CENTER_CENTER, text, font.clone(), Color32::from_black_alpha(220));
    }
    // …then once in white on top.
    p.text(rect.center(), Align2::CENTER_CENTER, text, font, Color32::WHITE);
}

/// Small "GitHub" link with a hand-drawn git-branch icon. Returns the click response.
pub fn github_button(ui: &mut egui::Ui) -> egui::Response {
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
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text("Open the Vollded page on GitHub")
}

/// Green (valid) / red (invalid) outline around an input box. The clip area is made a little
/// wider than the card so the left and right edges are not cut off.
pub fn validity_outline(ui: &egui::Ui, rect: Rect, ok: bool) {
    let mut clip = ui.clip_rect();
    clip.min.x -= 8.0;
    clip.max.x += 8.0;
    ui.painter()
        .with_clip_rect(clip)
        .rect_stroke(rect.expand(2.0), 6.0, Stroke::new(2.0, if ok { GOOD } else { ERR }));
}

/// Small round "i" button (drawn with the painter, no icon font needed).
pub fn info_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
    let col = if resp.hovered() { ACCENT } else { MUTED };
    let p = ui.painter();
    let c = rect.center();
    p.circle_stroke(c, 8.5, Stroke::new(1.3, col)); // ring
    p.circle_filled(c + Vec2::new(0.0, -3.8), 1.2, col); // dot of the "i"
    p.line_segment([c + Vec2::new(0.0, -1.0), c + Vec2::new(0.0, 4.6)], Stroke::new(1.7, col)); // stem
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text("About Vollded UGC")
}

/// Small clickable text (turns red + underlined on hover). Returns the click response.
pub fn link_text(ui: &mut egui::Ui, text: &str, size: f32) -> egui::Response {
    // Color32::PLACEHOLDER = "use the color I pass to `galley()` later", so hover can change it.
    let galley = ui.painter().layout_no_wrap(text.to_owned(), FontId::proportional(size), Color32::PLACEHOLDER);
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::click());
    let col = if resp.hovered() { ACCENT } else { MUTED };
    let p = ui.painter();
    p.galley(rect.min, galley, col);
    if resp.hovered() {
        p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, col));
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Draws a little trash-can icon centered on `c` (no icon font needed).
pub fn draw_trash(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    let s = Stroke::new(1.4, col);
    p.line_segment([c + Vec2::new(-6.0, -4.0), c + Vec2::new(6.0, -4.0)], s); // lid
    p.line_segment([c + Vec2::new(-2.0, -4.0), c + Vec2::new(-2.0, -6.5)], s); // handle
    p.line_segment([c + Vec2::new(2.0, -4.0), c + Vec2::new(2.0, -6.5)], s);
    p.line_segment([c + Vec2::new(-2.0, -6.5), c + Vec2::new(2.0, -6.5)], s);
    p.rect_stroke(Rect::from_min_max(c + Vec2::new(-4.5, -3.0), c + Vec2::new(4.5, 6.5)), 1.5, s); // body
    p.line_segment([c + Vec2::new(-1.5, -0.5), c + Vec2::new(-1.5, 4.0)], s); // slats
    p.line_segment([c + Vec2::new(1.5, -0.5), c + Vec2::new(1.5, 4.0)], s);
}

/// Highlights a drop zone while files are being dragged over the window.
pub fn draw_drop_zone(p: &egui::Painter, rect: Rect, text: &str) {
    if rect.width() < 10.0 {
        return;
    }
    let r = rect.shrink(6.0);
    p.rect_filled(r, 14.0, Color32::from_rgba_unmultiplied(225, 48, 60, 40));
    p.rect_stroke(r, 14.0, Stroke::new(2.0, ACCENT));
    p.text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(16.0), TEXT);
}
