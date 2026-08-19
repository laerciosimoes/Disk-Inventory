import { reactive, ref } from "vue";
import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  DiskInfo,
  FsEntry,
  ScanMessage,
  TreeEntry,
} from "../types";

interface NodeState {
  children: TreeEntry[] | null;
  isLoading: boolean;
  error: string | null;
}

interface DirectoryContents {
  path: string;
  entries: FsEntry[];
  complete: boolean;
  generation: number;
}

const EMPTY_NODE: NodeState = {
  children: null,
  isLoading: false,
  error: null,
};

// -----------------------------------------------------------------------------
// Tree state
// -----------------------------------------------------------------------------

const nodes = reactive(new Map<string, NodeState>());
const expanded = reactive(new Set<string>());

const selectedPath = ref<string | null>(null);
const selectedIsDir = ref(false);
const hoveredPath = ref<string | null>(null);

const zoomRoot = ref<string | null>(null);
const zoomFloor = ref<string | null>(null);

// -----------------------------------------------------------------------------
// Scan state
// -----------------------------------------------------------------------------

const scanTotalBytes = ref(0);
const scanScannedBytes = ref(0);
const scanScannedFiles = ref(0);
const isScanning = ref(false);
const scanGeneration = ref(0);

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

function toTreeEntry(entry: FsEntry): TreeEntry {
  const trimmed = entry.path.endsWith("/")
    ? entry.path.slice(0, -1)
    : entry.path;

  const name =
    trimmed.slice(trimmed.lastIndexOf("/") + 1) ||
    trimmed;

  return {
    ...entry,
    name,
    isDir: entry.entryType === "directory",
    isSymlink: entry.entryType === "symlink",
  };
}

function peek(path: string): NodeState {
  return nodes.get(path) ?? EMPTY_NODE;
}

// -----------------------------------------------------------------------------
// Directory loading
// -----------------------------------------------------------------------------

/**
 * Loads a directory only once.
 *
 * The complete filesystem remains in Rust.
 * The frontend receives only the immediate children of this directory.
 */
async function ensureChildren(path: string): Promise<void> {
  const existing = nodes.get(path);

  // Already loaded or currently loading.
  if (
    existing &&
    (existing.children !== null || existing.isLoading)
  ) {
    return;
  }

  if (!existing) {
    nodes.set(path, { children: null, isLoading: false, error: null });
  }

  // Re-read through the reactive Map rather than reusing `existing`/the
  // object literal above directly: a plain object stored via `nodes.set()`
  // and then mutated through a separately-held reference bypasses Vue's
  // reactivity tracking for those mutations (the write never goes through
  // the reactive proxy `nodes.get()` returns), so callers watching this
  // node's `isLoading`/`children` never get notified. Mutating through the
  // value `nodes.get()` gives back avoids that trap.
  const node = nodes.get(path);
  if (!node) {
    return;
  }

  node.isLoading = true;
  node.error = null;

  try {
    const result = await invoke<DirectoryContents>(
      "get_directory_contents",
      { path },
    );

    node.children = result.entries.map(toTreeEntry);
  } catch (err) {
    node.error = String(err);
    node.children = [];
  } finally {
    node.isLoading = false;
  }
}

/**
 * Re-fetches an already-loaded directory.
 *
 * Used while the Rust scanner is still discovering files.
 *
 * We deliberately do not toggle isLoading so the TreeView doesn't flicker.
 */
async function refreshChildren(path: string): Promise<void> {
  const node = nodes.get(path);

  if (!node || node.children === null) {
    return;
  }

  try {
    const result = await invoke<DirectoryContents>(
      "get_directory_contents",
      { path },
    );

    node.children = result.entries.map(toTreeEntry);
  } catch {
    // Keep the last known good result.
  }
}

/**
 * Refresh only directories currently expanded in the TreeView.
 *
 * This is important:
 *
 * The filesystem may contain millions of files, but the frontend only
 * requests directories the user is currently exploring.
 */
function refreshExpandedNodes(): void {
  for (const path of expanded) {
    void refreshChildren(path);
  }
}

// -----------------------------------------------------------------------------
// Scan
// -----------------------------------------------------------------------------

async function startScan(rootPath: string): Promise<void> {
  if (isScanning.value) {
    return;
  }

  // Show the scan banner immediately rather than waiting for the "start"
  // message's IPC round-trip, so there's no gap where a scan is already
  // running but nothing on screen says so.
  isScanning.value = true;
  scanTotalBytes.value = 0;
  scanScannedBytes.value = 0;
  scanScannedFiles.value = 0;

  // Make sure the root directory is immediately available to the TreeView.
  void ensureChildren(rootPath);

  const disks = await invoke<DiskInfo[]>("list_disks");

  const channel = new Channel<ScanMessage>();

  channel.onmessage = (message) => {
    switch (message.type) {
      case "start": {
        scanTotalBytes.value = message.data.totalBytes;
        scanScannedBytes.value = 0;
        scanScannedFiles.value = 0;
        scanGeneration.value = message.data.generation;
        isScanning.value = true;

        break;
      }

      case "progress": {
        // Ignore messages from an older scan.
        if (
          scanGeneration.value !== 0 &&
          message.data.generation !== scanGeneration.value
        ) {
          return;
        }

        scanScannedBytes.value =
          message.data.scannedBytes;

        scanScannedFiles.value =
          message.data.scannedFiles;

        /*
         * Rust owns the complete index.
         *
         * We refresh directories the user has expanded, plus the scan
         * root and the treemap's current zoom target — otherwise those
         * two never move past their very first (usually near-empty)
         * snapshot from the instant the scan started.
         */
        refreshExpandedNodes();
        void refreshChildren(rootPath);

        if (zoomRoot.value && zoomRoot.value !== rootPath) {
          void refreshChildren(zoomRoot.value);
        }

        break;
      }

      case "complete": {
        if (
          scanGeneration.value !== 0 &&
          message.data.generation !== scanGeneration.value
        ) {
          return;
        }

        /*
         * One final refresh guarantees that the currently visible
         * directories — including the scan root and the treemap's zoom
         * target — contain the final scan results.
         */
        refreshExpandedNodes();
        void refreshChildren(rootPath);

        if (zoomRoot.value && zoomRoot.value !== rootPath) {
          void refreshChildren(zoomRoot.value);
        }

        isScanning.value = false;

        break;
      }
    }
  };

  try {
    await invoke("scan_directory", {
      path: rootPath,
      disks,
      channel,
    });
  } catch (err) {
    console.error("Failed to scan directory:", err);
    isScanning.value = false;
  }
}

// -----------------------------------------------------------------------------
// Expansion
// -----------------------------------------------------------------------------

function isExpanded(path: string): boolean {
  return expanded.has(path);
}

async function setExpanded(
  path: string,
  value: boolean,
): Promise<void> {
  if (value) {
    expanded.add(path);

    await ensureChildren(path);
  } else {
    expanded.delete(path);
  }
}

async function toggleExpanded(path: string): Promise<void> {
  await setExpanded(
    path,
    !expanded.has(path),
  );
}

// -----------------------------------------------------------------------------
// Selection / hover
// -----------------------------------------------------------------------------

function select(
  path: string | null,
  isDir = false,
): void {
  selectedPath.value = path;
  selectedIsDir.value = isDir;
}

function hover(path: string | null): void {
  hoveredPath.value = path;
}

// -----------------------------------------------------------------------------
// Path helpers
// -----------------------------------------------------------------------------

function parentPath(path: string): string {
  const trimmed = path.endsWith("/")
    ? path.slice(0, -1)
    : path;

  const idx = trimmed.lastIndexOf("/");

  return idx > 0
    ? trimmed.slice(0, idx)
    : trimmed;
}

// -----------------------------------------------------------------------------
// Zoom
// -----------------------------------------------------------------------------

/**
 * Establishes the volume root as the lowest zoom level.
 */
function initZoom(rootPath: string): void {
  if (zoomFloor.value === rootPath) {
    return;
  }

  zoomFloor.value = rootPath;
  zoomRoot.value = rootPath;
}

async function zoomIn(path: string): Promise<void> {
  await setExpanded(path, true);

  zoomRoot.value = path;
}

function zoomOut(): void {
  if (
    !zoomRoot.value ||
    zoomRoot.value === zoomFloor.value
  ) {
    return;
  }

  zoomRoot.value = parentPath(
    zoomRoot.value,
  );
}

function canZoomOut(): boolean {
  return (
    !!zoomRoot.value &&
    zoomRoot.value !== zoomFloor.value
  );
}

/**
 * Jumps straight back to the volume root, regardless of how many levels
 * deep the treemap is currently zoomed into.
 */
function zoomToRoot(): void {
  if (!zoomFloor.value) {
    return;
  }

  zoomRoot.value = zoomFloor.value;
}

function canZoomInSelected(): boolean {
  return (
    selectedIsDir.value &&
    !!selectedPath.value &&
    selectedPath.value !== zoomRoot.value
  );
}

async function zoomInSelected(): Promise<void> {
  if (
    !canZoomInSelected() ||
    !selectedPath.value
  ) {
    return;
  }

  await zoomIn(selectedPath.value);
}

// -----------------------------------------------------------------------------
// Reset
// -----------------------------------------------------------------------------

/**
 * Clears frontend-only state.
 *
 * IMPORTANT:
 * This does not affect the Rust filesystem index.
 * The next scan will replace the Rust index.
 */
function reset(): void {
  nodes.clear();
  expanded.clear();

  selectedPath.value = null;
  selectedIsDir.value = false;
  hoveredPath.value = null;

  zoomRoot.value = null;
  zoomFloor.value = null;

  scanTotalBytes.value = 0;
  scanScannedBytes.value = 0;
  scanScannedFiles.value = 0;
  scanGeneration.value = 0;
  isScanning.value = false;
}

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------

/**
 * One module-level store per Tauri webview context.
 *
 * The Rust process owns the complete filesystem index.
 * This store owns only the directories that the frontend has loaded.
 */
export function useFsTree() {
  return {
    // Tree state
    expanded,
    selectedPath,
    selectedIsDir,
    hoveredPath,
    zoomRoot,

    // Scan state
    scanTotalBytes,
    scanScannedBytes,
    scanScannedFiles,
    scanGeneration,
    isScanning,

    // Tree operations
    peek,
    ensureChildren,
    refreshChildren,
    refreshExpandedNodes,

    // Scan
    startScan,

    // Expansion
    isExpanded,
    setExpanded,
    toggleExpanded,

    // Selection
    select,
    hover,

    // Zoom
    initZoom,
    zoomIn,
    zoomOut,
    zoomToRoot,
    canZoomOut,
    canZoomInSelected,
    zoomInSelected,

    // Path helpers
    parentPath,

    // Reset
    reset,
  };
}