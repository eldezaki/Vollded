// ════════════════════════════════════════════════════════════════════════════
//  Vollded UGC — a portable manager for Roblox UGC project folders.
//
//  Where to find things:
//    main.rs        start-up only: window options, icon, crash log
//    app.rs         the `App` struct (all state) and `update()`, called every frame
//    app/           the rest of `impl App`: panels, dialogs, drag & drop, reordering
//    detail.rs      the detail view (cards for the selected UGC)
//    widgets.rs     small reusable UI pieces
//    model.rs       data types: Status, Meta, Item
//    ops.rs         file operations (rename, copy dropped files, Recycle Bin)
//    util.rs        helpers for reading the folder
//    config.rs      portable settings next to the exe
//    theme.rs       colors and egui style
//    platform.rs    window icon, cursor position while dragging files
//    constants.rs   limits, links, the About components list
// ════════════════════════════════════════════════════════════════════════════

// On Windows release builds, don't pop up a black console window behind the app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Each `mod` line adds one file (src/<name>.rs) to the program.
mod app;
mod config;
mod constants;
mod detail;
mod model;
mod ops;
mod platform;
mod theme;
mod util;
mod widgets;

use crate::app::App;
use crate::config::exe_dir;
use crate::platform::load_icon;
use eframe::egui;
use std::fs;

/// Program entry point.
fn main() -> eframe::Result<()> {
    // If the app ever panics, write the reason next to the exe (no console window to read it from).
    std::panic::set_hook(Box::new(|info| {
        let _ = fs::write(exe_dir().join("vollded_ugc_crash.log"), format!("{info}\n"));
    }));
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([980.0, 600.0])
        .with_min_inner_size([980.0, 600.0])
        .with_title("Vollded UGC");
    // Window + taskbar icon (baked into the exe from assets/icon.png).
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        // Windowed apps are composed by the OS, so there is no visible tearing; turning vsync
        // off stops the graphics driver from busy-waiting. The frame limiter paces frames instead.
        vsync: false,
        ..Default::default() // every other option keeps its default value
    };
    // `Box::new(|cc| …)` is the function eframe calls to build our `App`.
    eframe::run_native(
        "Vollded UGC",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
