# Disk Inventory

A desktop app for macOS that shows you what's actually taking up space on your disks — a lazy-loading folder tree and a zoomable, color-coded treemap, both updating live while a background scan works through the volume.

Built with [Tauri 2](https://tauri.app/) (Rust backend) and [Vue 3](https://vuejs.org/) + TypeScript (frontend).

## Features

- **Volume picker** — lists every mounted disk (name, filesystem, used/total space, removable/read-only flags) and lets you open any of them for inspection.
- **Live background scanning** — opening a volume kicks off a full recursive scan on a background thread. The folder tree and treemap start populating within about a second and keep updating roughly twice a second as the scan discovers more of the disk — you don't wait for the whole scan to finish before seeing results.
- **Folder tree** — classic expand/collapse view, sorted largest-first at every level, with a pinned root row that also doubles as a "back to root" control for the treemap.
- **Zoomable treemap** — a squarified treemap (Bruls/Huizing/van Wijk layout) of the currently focused folder. Double-click any tile to zoom into it; the tree's root row zooms back out.
- **Color-coded by content** — files are colored by type (images, video, audio, documents, spreadsheets, archives, code, apps, fonts), so you can spot what's eating your disk at a glance. Folders each get their own distinct, muted color so siblings are easy to tell apart.
- **Physical disk usage, not logical size** — sizes reflect actual allocated disk blocks (`st_blocks`), not just each file's apparent byte length. This matters a lot on APFS, where copy-on-write clones (heavily used under `~/Library/Group Containers` by Mail, Photos, Messages, etc.) can make logical-size sums wildly exceed what's physically on disk.
- **Resizable layout** — drag the divider between the folder tree and the treemap to resize either pane.
- **Multi-window** — each opened volume gets its own native window; closing one returns you to the volume picker rather than quitting the app.

## Requirements

- macOS (the app currently targets macOS only — QoS thread tuning and some path-exclusion logic are macOS-specific)
- [Node.js](https://nodejs.org/) and [pnpm](https://pnpm.io/) (`tauri dev`/`tauri build` shell out to `pnpm`, regardless of whether you invoke them via `npm` or `pnpm` — see [Notes](#notes))
- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- Xcode Command Line Tools (`xcode-select --install`), required by Tauri on macOS

## Getting started

```bash
npm install          # install JS dependencies
npm run tauri dev    # start the dev server and launch the app
```

The first window lists your mounted volumes. Click one to open a dedicated window for it and start scanning.

## Building a release bundle

```bash
npm run tauri build
```

Produces a signed/notarizable `.app` (and other configured bundle targets) under `src-tauri/target/release/bundle/`.

## Commands reference

| Command | What it does |
|---|---|
| `npm install` | Install JS dependencies |
| `npm run tauri dev` | Run the app in dev mode (Vite + `cargo run`, hot-reloading) |
| `npm run build` | Typecheck (`vue-tsc --noEmit`) and build the frontend only |
| `npm run tauri build` | Full production app bundle |
| `npx vue-tsc --noEmit` | Typecheck the Vue/TypeScript frontend |
| `cargo check --manifest-path src-tauri/Cargo.toml` | Typecheck the Rust backend |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Run the Rust unit test suite |
| `cargo run --manifest-path src-tauri/Cargo.toml --bin scratch -- <path>` | Ad-hoc: run the real scan engine against a real path and print progress to the terminal, no GUI |

## How it works

**Backend (Rust, `src-tauri/src/`)** — every filesystem/disk operation is a `#[tauri::command]`; the frontend never touches the filesystem directly.

- `disks.rs` enumerates mounted volumes via the [`sysinfo`](https://crates.io/crates/sysinfo) crate.
- `filesystem.rs` is the scan engine. `scan_directory` walks the chosen root in parallel with [`jwalk`](https://crates.io/crates/jwalk) on a background thread, bounded to the root's own filesystem device so it never crosses into another mounted volume or loops through macOS's self-referential mounts (e.g. `/Volumes/Macintosh HD -> /`). As it walks, it builds a complete in-memory index — including a running recursive size per directory — behind a lock, committing batched updates roughly every 250ms and pushing lifecycle/progress events to the frontend over a Tauri `Channel` roughly every 500ms. `get_directory_contents` is the cheap, lazy read side: given a path, it returns whatever's currently known about that directory's children, patched with the latest running sizes — this is what lets the tree and treemap show live, partial results throughout a long scan instead of only after it finishes.
- `windows.rs` handles opening a volume window (closing the picker window first) and closing a volume window (reopening the picker), always creating the new window before closing the old one so the app is never briefly windowless.
- `lib.rs` wires everything into the Tauri app and caps the scan's thread pool (a quarter of the machine's cores, clamped to 2–4, run at background QoS priority on macOS) so a large scan can't starve the window's own rendering.

**Frontend (Vue 3 + TypeScript, `src/`)** — one Vue app that renders differently depending on which OS window it's running in (there's no router; window identity decides the view).

- `composables/fsTree.ts` is the reactive store backing a volume window: it mirrors only the directories currently visible on screen (never the whole index, which stays in Rust), and re-fetches the visible ones every time a scan progress message arrives.
- `components/FileTree.vue` / `TreeNode.vue` — the folder tree, lazy-expanding per node.
- `components/TreemapPanel.vue` / `TreemapNode.vue` — the treemap, using `utils/treemap.ts`'s squarified layout algorithm and `utils/treemapColor.ts`'s file-type/folder color scheme.
- `components/DiskList.vue` / `VolumeWindow.vue` — the two top-level views (volume picker and volume inspector).

For the full architectural deep-dive (module-by-module, including internal invariants like the device-boundary walking logic and why the frontend never eagerly loads the whole tree), see [`CLAUDE.md`](./CLAUDE.md).

## Testing

- **Rust**: `cargo test --manifest-path src-tauri/Cargo.toml` — unit tests cover the scan engine (including real temp-directory integration tests against the actual `jwalk` walker), the disk-listing logic, and window-state management. Rust command handlers that require a live Tauri app/window to construct (`State`, `AppHandle`, `Window`) are exercised through their extracted, pure inner logic instead, since they can't be unit-tested directly.
- **Frontend**: there is no JS/Vue test runner configured; `npx vue-tsc --noEmit` is the only automated frontend check. Verify UI changes by running the app (`npm run tauri dev`).

## Notes

- `src-tauri/tauri.conf.json`'s `beforeDevCommand`/`beforeBuildCommand` invoke `pnpm run dev` / `pnpm run build` — `tauri dev`/`tauri build` shell out to those regardless of whether you launch via `npm run tauri dev`, so `pnpm` needs to be installed even though the committed lockfile is npm's.
- Computing recursive folder sizes is inherently slow on directories with very many files (e.g. package-manager caches) since every file has to be `stat`'d — this is expected, not a bug.
- `src-tauri/src/bin/scratch.rs` is a throwaway dev tool, not a test — feel free to rewrite it to call whatever you're currently working on.
