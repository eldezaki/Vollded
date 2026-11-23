//! Colors and the egui style: a dark theme with a red accent.

use eframe::egui::{self, Color32, Stroke, Vec2};

pub const BG: Color32 = Color32::from_rgb(15, 15, 18);
pub const PANEL: Color32 = Color32::from_rgb(21, 21, 26);
pub const CARD: Color32 = Color32::from_rgb(31, 31, 38);
pub const CARD_HOVER: Color32 = Color32::from_rgb(42, 42, 52);
pub const ACCENT: Color32 = Color32::from_rgb(225, 48, 60); // the red theme color
pub const GOOD: Color32 = Color32::from_rgb(52, 211, 153); // green
pub const WARN: Color32 = Color32::from_rgb(251, 191, 36); // yellow
pub const ERR: Color32 = Color32::from_rgb(255, 80, 80); // bright red for invalid input
pub const BAD: Color32 = Color32::from_rgb(120, 126, 148); // grey used for "missing" marks
pub const TEXT: Color32 = Color32::from_rgb(232, 234, 242);
pub const MUTED: Color32 = Color32::from_rgb(138, 145, 168);

/// Applies our dark/red look to egui. Called once at startup.
pub fn setup_style(ctx: &egui::Context) {
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
