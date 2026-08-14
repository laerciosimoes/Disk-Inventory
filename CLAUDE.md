# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Tauri 2 desktop app (macOS) for inspecting disk usage: Vue 3 + TypeScript frontend, Rust backend. First screen lists mounted volumes; selecting one closes that window and opens a dedicated window showing a lazy-loading folder treeview for that volume, sorted by size (largest first).

## Commands

```bash
npm install              # install JS deps
npm run tauri dev        # run the app in dev mode (starts Vite + cargo run, opens the window)
npm run build             # vue-tsc --noEmit, then vite build (frontend only)
npm run tauri build       # full production app bundle

npx vue-tsc --noEmit                          # typecheck the Vue/TS frontend
cargo check --manifest-path src-tauri/Cargo.toml    # typecheck the Rust backend
cargo test --manifest-path src-tauri/Cargo.toml     # run Rust unit tests
cargo test --manifest-path src-tauri/Cargo.toml <test_name>   # run a single test

cargo run --manifest-path src-tauri/Cargo.toml --bin scratch -- <path>   # ad-hoc: call real code and print output, no GUI
```

`src-tauri/src/bin/scratch.rs` is a throwaway runner, not a test — edit its `main()` to call whatever function you're currently working on (from `filesystem`, `disks`, etc., all `pub mod` off the `disk_inventory_scaffold_lib` crate) and `cargo run` it to see real output on real paths. Rewrite it freely between runs; nothing in it is meant to be kept.

There is no JS/Vue test suite or linter configured — `vue-tsc --noEmit` is the only frontend check. Rust correctness for the filesystem-walking logic is covered by `#[cfg(test)]` unit tests in `src-tauri/src/filesystem.rs`.

When testing the running app, prefer `cargo test` over driving the GUI: this repo runs on a real developer machine (not a disposable sandbox), and OS-level window focus/multi-monitor coordinates are unreliable to script — don't fight it with pixel-coordinate GUI automation.

## Architecture

**Every filesystem/disk operation is a Rust `#[tauri::command]`, invoked from Vue via `@tauri-apps/api/core`'s `invoke()`.** The frontend never touches the filesystem directly. Commands live in `src-tauri/src/`:

- `disks.rs` — `list_disks`: enumerates mounted volumes via the `sysinfo` crate (name, mount point, filesystem, total/used/available bytes, removable/read-only flags).
- `filesystem.rs` — `list_directory`: lists one directory level. For each subdirectory it computes the **full recursive size** in parallel (via `rayon`), then sorts all entries (files and folders together) by size descending. Recursion is bounded to the directory's own filesystem device (`st_dev`), so it never crosses into another mounted volume or loops through macOS's self-referential mounts (e.g. `/Volumes/Macintosh HD -> /`) — this is the mechanism that makes it safe against symlink/mount cycles; see the tests in this file before changing the walking logic.
- `windows.rs` — multi-window orchestration: `open_volume_window` creates a new labeled window for a chosen mount point and closes the invoking window (new window is always created *before* the old one closes, so the app is never briefly windowless); `get_window_mount_point` lets a window ask "which volume am I showing" (backed by a `Mutex<HashMap<label, mount_point>>` app-managed state, cleaned up on window-destroy); `close_current_window` backs the Close button.
- `lib.rs` — wires the above into `tauri::Builder`, registers the window-destroyed cleanup hook, and lists every command in `invoke_handler!`.

Custom commands are not gated by Tauri's capability/permission system (only built-in core/plugin APIs are), so adding a new `#[tauri::command]` does not require touching `src-tauri/capabilities/default.json`.

**Frontend is single-page but multi-window**: there's one Vue app (`src/App.vue`), but it renders differently depending on which physical OS window it's running in. On mount, `App.vue` calls `get_window_mount_point`; if it gets nothing back it's the main picker window (`DiskList.vue`), if it gets a path back it's a volume window (`VolumeWindow.vue` → `FileTree.vue`). There is no router — window identity, not a URL, decides what's rendered.

- `components/DiskList.vue` — main window: lists volumes, calls `open_volume_window` on click (does not render a tree itself).
- `components/VolumeWindow.vue` — dedicated per-volume window chrome (title + Close button) wrapping `FileTree.vue`.
- `components/FileTree.vue` — loads and renders the root-level listing for a given path.
- `components/TreeNode.vue` — recursive, lazy-expanding node. Vue SFCs self-register by filename, so `TreeNode.vue` can reference itself in its own template without explicit registration. Each folder only calls `list_directory` for its children when first expanded — never walk the whole tree eagerly.
- `types.ts` / `utils/format.ts` — shared `DiskInfo`/`FsEntry` types (camelCase, matching `#[serde(rename_all = "camelCase")]` on the Rust structs) and byte-formatting helper.

## Notes

- Computing recursive folder sizes is inherently slow on directories with very many files (e.g. package-manager caches under a home directory) since it has to stat everything underneath — this is expected, not a bug to fix reactively.
- `tauri.conf.json` defines only the main window (900x900); volume windows are created dynamically at runtime from `windows.rs`, not declared statically.
