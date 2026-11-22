# Vollded UGC

My own personal portable, Rust-only (egui) manager for Vollded's Roblox UGC project folders.

## Portable
- One self-contained exe.
- Settings are stored in `vollded_ugc.json` **next to the exe**.
  If the Vollded folder is inside the exe's folder, the path is saved relative, so moving both together keeps working.
- If a folder named `Vollded` sits next to the exe, it is loaded automatically on first run.
- Statuses and notes live in `.ugc_manager.json` inside the Vollded folder, so they travel with the files.

## Folder layout
| Item | Detected as |
|---|---|
| `previews/*.mp4` | high-poly showcase videos |
| `textures/` | Substance Painter exports |
| `<id>.blend` | Model (raw high-poly) |
| `<id>_Ready.blend` | Ready (Roblox-import-ready) |
| `*.jpeg/jpg/png` | Reference image |
| `*.spp` (non-autosave) | Texture (Substance Painter project) |
| `*.txt` | file name = UGC name (max 50 chars, editable via Rename), content = description |

## Drag & drop
Files dropped on the window are **copied**, the originals are left untouched.

| Dropped on | Result |
|---|---|
| the **left list** | creates the next numbered folder (e.g. `4`) and files the dropped items into it |
| the **UGC view** | adds the files to the selected UGC, with a confirmation pop-up before anything is replaced |

| Extension | Stored as |
|---|---|
| `.blend` | Model (`<id>.blend`), never the Ready file |
| `.spp` | Texture (`<id>.spp`) |
| `.jpg` / `.jpeg` | Reference image |
| `.txt` | Description (the file name becomes the UGC name) |
| `.mp4` | `previews/` |
| `.png` | `textures/` |
| anything else | rejected |

## Features
- Status per UGC (To do, Modeling, Texturing, Ready, Uploaded) with a drop-down filter and search.
- Checklist and progress bar per UGC: Reference image, Model, Texture, Ready, Description (only counted if the `.txt` has text).
- Rename with a 50 character limit: the outline turns red while too long and green once valid.
- Editable description (saved back to the `.txt`, max 1000 characters) and private notes.
- Folder size on every list card and the total Vollded folder size in the bottom bar ("Folder size", clickable, opens the folder).
- Setting a UGC to Uploaded requires the link to the published UGC; a button next to Open folder opens it.
- Previews and Textures titles open their folders, every file has its own Open button.
- UGC cards can be dragged to reorder; folders and the files inside are renamed to match the new order (e.g. `2` becomes `1`).
- Delete button on every card moves the whole UGC folder to the Recycle Bin after a confirmation.
- Nothing is ever deleted permanently: replaced files and deleted UGCs all go to the Recycle Bin.
- About pop-up (info button, bottom right) with author, version and the list of components used.
- A crash log (`vollded_ugc_crash.log`) is written next to the exe if the app ever panics.
