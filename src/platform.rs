//! Things that depend on the operating system or on embedded assets: the window icon
//! and the mouse position while files are dragged in from Explorer.

use eframe::egui;

/// Loads the window icon from assets/icon.png. `include_bytes!` bakes the file INTO the exe at
/// compile time, so the exe stays a single portable file.
pub fn load_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png")).ok()?.to_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData { rgba: img.into_raw(), width, height })
}

/// Windows only: ask the OS where the mouse cursor is. We declare the one function we need
/// ourselves, so no extra crate is required. (While dragging files from Explorer, Windows
/// doesn't send mouse-move events to the app, so egui's own pointer position would be stale.)
#[cfg(windows)]
mod winapi_min {
    #[repr(C)] // lay the struct out in memory exactly like C does
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }
    #[link(name = "user32")]
    extern "system" {
        pub fn GetCursorPos(p: *mut Point) -> i32;
    }
    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut p = Point { x: 0, y: 0 };
        // Calling a foreign C function is `unsafe`: the compiler can't verify it for us.
        let ok = unsafe { GetCursorPos(&mut p) };
        if ok != 0 { Some((p.x, p.y)) } else { None }
    }
}

/// Where was the cursor (in egui coordinates, relative to our window) when files were dropped?
pub fn drop_pos(ctx: &egui::Context) -> Option<egui::Pos2> {
    #[cfg(windows)] // this block only exists in Windows builds
    {
        if let (Some((cx, cy)), Some(inner)) = (winapi_min::cursor_pos(), ctx.input(|i| i.viewport().inner_rect)) {
            let ppp = ctx.pixels_per_point(); // screen pixels per egui point (DPI scaling)
            return Some(egui::pos2(cx as f32 / ppp - inner.min.x, cy as f32 / ppp - inner.min.y));
        }
    }
    ctx.input(|i| i.pointer.hover_pos().or(i.pointer.latest_pos()))
}
