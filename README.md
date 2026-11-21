# Vollded UGC

Portable, Rust-only (egui) manager for Roblox UGC project folders.

    cargo build --release
    # -> target/release/vollded-ugc.exe  (single file, no installer)

## Portable
- One self-contained exe. Copy it anywhere (USB stick, another PC).
- Settings are stored in `vollded_ugc.json` **next to the exe**, never in AppData or the registry.
  If your Vollded folder is inside the exe's folder, the path is saved relative, so moving both together keeps working.
- If a folder named `Vollded` sits next to the exe, it is loaded automatically on first run.
- Statuses and notes live in `.ugc_manager.json` inside your Vollded folder, so they travel with your files.

## Folder layout it understands
| Item | Detected as |
|---|---|
| `previews/*.mp4` | high-poly showcase videos |
| `textures/` | Substance Painter exports |
| `<id>.blend` | Model (raw high-poly) |
| `<id>_Ready.blend` | Ready (Roblox-import-ready) |
| `*.jpeg/jpg/png` | Reference image |
| `*.spp` (non-autosave) | Texture (Substance Painter project) |
| `*.txt` | file name = UGC name (max 50 chars, editable via Rename), content = description |
