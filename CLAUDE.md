# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Tauri 2 desktop app (macOS) for inspecting disk usage: Vue 3 + TypeScript frontend, Rust backend. First screen lists mounted volumes; selecting one closes that window and opens a dedicated window that kicks off a background full-volume scan and shows the results as both a lazy-loading folder treeview and a zoomable treemap, sorted by size (largest first).

## Commands

```bash
npm install                     # install JS deps (package-lock.json is npm; there is no pnpm-lock.yaml)
npm run tauri dev               # run the app in dev mode (starts Vite + cargo run, opens the window)
npm run build                    # vue-tsc --noEmit, then vite build (frontend only)
npm run tauri build              # full production app bundle

npx vue-tsc --noEmit                                 # typecheck the Vue/TS frontend
cargo check --manifest-path src-tauri/Cargo.toml     # typecheck the Rust backend
cargo test --manifest-path src-tauri/Cargo.toml      # run Rust unit tests

cargo run --manifest-path src-tauri/Cargo.toml --bin scratch -- <path>   # ad-hoc: call real code and print output, no GUI
```

`src-tauri/tauri.conf.json`'s `beforeDevCommand`/`beforeBuildCommand` invoke `pnpm run dev` / `pnpm run build` (not npm) — `tauri dev`/`tauri build` shell out to those regardless of whether you launched via `npm run tauri dev`, so `pnpm` must be installed even though the committed lockfile is npm's.

There is no JS/Vue test suite or linter configured — `vue-tsc --noEmit` is the only frontend check. `src-tauri/src/filesystem.rs` currently has no `#[cfg(test)]` unit tests, despite being the module with the trickiest logic (device-boundary walking, ancestor size aggregation) — be relatively cautious about assuming behavior is covered.

`src-tauri/src/bin/scratch.rs` is a throwaway runner, not a test — edit its `main()` to call whatever function you're currently working on (from `filesystem`, `disks`, etc., all `pub mod` off the `disk_inventory_lib` crate) and `cargo run` it to see real output on real paths. Rewrite it freely between runs; nothing in it is meant to be kept. As of now it does not compile — it matches an older `ScanMessage` shape (a since-removed `Entries` variant, `Start`/`Progress`/`Complete` without the `generation` field) — fix it up to match the current enum in `filesystem.rs` before relying on it.

When testing the running app, prefer `cargo test` over driving the GUI: this repo runs on a real developer machine (not a disposable sandbox), and OS-level window focus/multi-monitor coordinates are unreliable to script — don't fight it with pixel-coordinate GUI automation.

## Architecture

**Every filesystem/disk operation is a Rust `#[tauri::command]`, invoked from Vue via `@tauri-apps/api/core`'s `invoke()`.** The frontend never touches the filesystem directly. Commands live in `src-tauri/src/`:

- `disks.rs` — `list_disks`: enumerates mounted volumes via the `sysinfo` crate (name, mount point, filesystem, total/used/available bytes, removable/read-only flags), filtering out macOS's internal `/System/Volumes/*` mounts.
- `filesystem.rs` — the scan engine. A volume window's whole session is one full background scan plus cheap reads against its in-memory result:
  - `scan_directory` (async command) spawns `scan_directory_internal` on a blocking thread. It walks the chosen root with `jwalk` in parallel, bounded to the root's own device (`st_dev`, via each entry's `metadata().dev()`) so it never crosses into another mounted volume or loops through macOS's self-referential mounts (e.g. `/Volumes/Macintosh HD -> /`). When the root is `/`, `should_skip_directory` additionally excludes other mounted volumes and the synthetic `/System/Volumes/Data`. As it walks, it builds a complete `ScanIndex` (children-by-parent map, plus a running recursive size per directory computed by walking each file's path up to ancestors — `add_size_to_ancestors`) behind `AppState.scan_results: Arc<RwLock<ScanIndex>>`, committing batched updates roughly every 250ms and pushing `ScanMessage::{Start,Progress,Complete}` lifecycle events over a Tauri `Channel` roughly every 500ms. Each scan has a `generation` counter so stale messages from a superseded scan can be ignored.
  - `get_directory_contents` (sync command) is the lazy per-level read: given a path, returns that directory's already-known children from `ScanIndex` (patched with the latest running sizes), sorted by size descending. This is what the treeview and treemap actually call — they never see the raw scan stream, only whatever's currently in the index for the paths they ask about.
  - **Known gap**: `AppState` is never `.manage()`'d in `lib.rs`'s `tauri::Builder` (only `VolumeWindowState` is) — as of this writing, invoking `scan_directory` or `get_directory_contents` will panic with a "state not managed" error. Check this first if either command fails at runtime.
- `windows.rs` — multi-window orchestration: `open_volume_window` creates a new labeled window for a chosen mount point and closes the invoking window (new window is always created *before* the old one closes, so the app is never briefly windowless); `get_window_mount_point` lets a window ask "which volume am I showing" (backed by a `Mutex<HashMap<label, mount_point>>` app-managed state, cleaned up on window-destroy); `close_current_window` backs the Close button.
- `lib.rs` — wires the above into `tauri::Builder`, registers the window-destroyed cleanup hook, lists every command in `invoke_handler!`, and calls `init_bounded_rayon_pool()` before starting: it caps rayon's global thread pool to `cores/4` (clamped 2–4) and, on macOS, marks those worker threads background-QoS via a direct `pthread_set_qos_class_self_np` call (no extra dependency). This keeps a large recursive scan from starving the webview's own render/compositor thread — the workload is `stat` syscalls, not compute, so past a modest thread count more parallelism mostly adds VFS lock contention rather than throughput anyway.

Custom commands are not gated by Tauri's capability/permission system (only built-in core/plugin APIs are — see `capabilities/default.json`), so adding a new `#[tauri::command]` does not require touching it.

**Frontend is single-page but multi-window**: there's one Vue app (`src/App.vue`), but it renders differently depending on which physical OS window it's running in. On mount, `App.vue` calls `get_window_mount_point`; if it gets nothing back it's the main picker window (`DiskList.vue`), if it gets a path back it's a volume window (`VolumeWindow.vue`).  There is no router — window identity, not a URL, decides what's rendered.

- `composables/fsTree.ts` (`useFsTree`) — the module-level singleton store backing a volume window (state is declared at module scope with `reactive`/`ref`, so every `useFsTree()` call in that webview shares one instance; each OS window is its own JS context, so this doesn't leak across windows). It owns:
  - **Tree state**: a `Map<path, NodeState>` of only the directories the user has actually expanded (`ensureChildren`/`refreshChildren`), plus `expanded`, `selectedPath`, `hoveredPath` — Rust holds the full index, the frontend only mirrors what's currently visible.
  - **Scan lifecycle**: `startScan` opens a `Channel`, invokes `scan_directory`, and on each `progress`/`complete` message calls `refreshExpandedNodes()` to re-fetch (not re-toggle-loading, to avoid flicker) every currently-expanded directory from `get_directory_contents` — this is how the tree and treemap update live while a scan is still running.
  - **Zoom**: `zoomRoot`/`zoomFloor` track the treemap's current focus node and the volume root floor it can't zoom out past; `zoomIn`/`zoomOut`/`zoomInSelected` drive it.
- `components/DiskList.vue` — main window: lists volumes, calls `open_volume_window` on click (does not render a tree itself).
- `components/VolumeWindow.vue` — dedicated per-volume window chrome: starts the scan (`tree.startScan`) on mount, shows a `ProgressBar` banner and status-bar file/byte counters while scanning, and lays out `FileTree` (fixed-width pane) beside `TreemapPanel` (flex-fill) plus a `TreemapLegend` shown once scanning finishes.
- `components/FileTree.vue` / `TreeNode.vue` — the classic disclosure treeview. `TreeNode` is recursive and lazy-expanding (Vue SFCs self-register by filename, so it can reference itself in its own template without explicit registration); each folder only calls `get_directory_contents` for its children when first expanded — never walk the whole tree eagerly.
- `components/TreemapPanel.vue` / `TreemapNode.vue` — the squarified-treemap view of whatever `tree.zoomRoot` currently points at. `TreemapNode` recursively renders its own expanded children as nested absolutely-positioned boxes; double-click zooms in (`emit('zoom', path)`), colored via `utils/treemapColor.ts`'s deterministic hue-per-top-level-branch hash so a folder keeps a stable color as you navigate.
- `components/ProgressBar.vue` — shared indeterminate/determinate progress bar used by both the scan banner and per-node loading states.
- `utils/treemap.ts` — `squarify`, the Bruls/Huizing/van Wijk squarified treemap layout algorithm, operating in 0–100 percentage units so it nests for free under CSS percentage positioning.
- `types.ts` / `utils/format.ts` — shared `DiskInfo`/`FsEntry`/`TreeEntry`/`ScanMessage` types (camelCase, matching `#[serde(rename_all = "camelCase")]` on the Rust structs) and byte-formatting helper.

## Notes

- Computing recursive folder sizes is inherently slow on directories with very many files (e.g. package-manager caches under a home directory) since it has to stat everything underneath — this is expected, not a bug to fix reactively.
- `tauri.conf.json` defines only the main window (900x900); volume windows are created dynamically at runtime from `windows.rs`, not declared statically.
