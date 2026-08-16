import { reactive, ref } from "vue";
import { Channel, invoke } from "@tauri-apps/api/core";
import type { DiskInfo, FsEntry, ScanMessage, TreeEntry } from "../types";

interface NodeState {
  children: TreeEntry[] | null;
  isLoading: boolean;
  error: string | null;
}

const EMPTY_NODE: NodeState = { children: null, isLoading: false, error: null };

const nodes = reactive(new Map<string, NodeState>());
const expanded = reactive(new Set<string>());
const selectedPath = ref<string | null>(null);
const selectedIsDir = ref(false);
const hoveredPath = ref<string | null>(null);
const zoomRoot = ref<string | null>(null);
const zoomFloor = ref<string | null>(null);

const scanTotalBytes = ref(0);
const scanScannedBytes = ref(0);
const scanScannedFiles = ref(0);
const isScanning = ref(false);

function toTreeEntry(entry: FsEntry): TreeEntry {
  const trimmed = entry.path.endsWith("/") ? entry.path.slice(0, -1) : entry.path;
  const name = trimmed.slice(trimmed.lastIndexOf("/") + 1);
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

async function ensureChildren(path: string): Promise<void> {
  const existing = nodes.get(path);
  if (existing && (existing.children !== null || existing.isLoading)) return;

  const node: NodeState = existing ?? { children: null, isLoading: false, error: null };
  nodes.set(path, node);
  node.isLoading = true;
  node.error = null;
  try {
    const entries = await invoke<FsEntry[]>("get_directory_contents", { path });
    node.children = entries.map(toTreeEntry);
  } catch (err) {
    node.error = String(err);
    node.children = [];
  } finally {
    node.isLoading = false;
  }
}

/** Re-fetches an already-loaded node's children in place, without touching
 * isLoading/error, so visible rows don't flicker back to "Loading..." while
 * a scan is progressively filling in more accurate results. */
async function refreshChildren(path: string): Promise<void> {
  const node = nodes.get(path);
  if (!node || node.children === null) return;
  try {
    const entries = await invoke<FsEntry[]>("get_directory_contents", { path });
    node.children = entries.map(toTreeEntry);
  } catch {
    // Keep the last-known-good children; a transient refresh failure
    // shouldn't blank out already-rendered rows.
  }
}

function refreshLoadedNodes(): void {
  for (const path of nodes.keys()) {
    void refreshChildren(path);
  }
}

async function startScan(rootPath: string): Promise<void> {
  if (isScanning.value) return;

  void ensureChildren(rootPath);

  const disks = await invoke<DiskInfo[]>("list_disks");

  const channel = new Channel<ScanMessage>();
  channel.onmessage = (message) => {
    if (message.type === "start") {
      scanTotalBytes.value = message.data.totalBytes;
      scanScannedBytes.value = 0;
      scanScannedFiles.value = 0;
      isScanning.value = true;
    } else if (message.type === "progress") {
      scanScannedBytes.value = message.data.scannedBytes;
      scanScannedFiles.value = message.data.scannedFiles;
      refreshLoadedNodes();
    } else if (message.type === "complete") {
      scanScannedBytes.value = scanTotalBytes.value;
      refreshLoadedNodes();
      isScanning.value = false;
    }
  };

  try {
    await invoke("scan_directory", { path: rootPath, disks, channel });
  } catch {
    isScanning.value = false;
  }
}

function isExpanded(path: string): boolean {
  return expanded.has(path);
}

async function setExpanded(path: string, value: boolean): Promise<void> {
  if (value) {
    expanded.add(path);
    await ensureChildren(path);
  } else {
    expanded.delete(path);
  }
}

async function toggleExpanded(path: string): Promise<void> {
  await setExpanded(path, !expanded.has(path));
}

function select(path: string | null, isDir = false) {
  selectedPath.value = path;
  selectedIsDir.value = isDir;
}

function hover(path: string | null) {
  hoveredPath.value = path;
}

function parentPath(path: string): string {
  const trimmed = path.endsWith("/") ? path.slice(0, -1) : path;
  const idx = trimmed.lastIndexOf("/");
  return idx > 0 ? trimmed.slice(0, idx) : trimmed;
}

/** Establishes the volume's root as the floor zoom can't rise above. A no-op past the first call for a given root, so remounts don't reset an in-progress zoom. */
function initZoom(rootPath: string): void {
  if (zoomFloor.value === rootPath) return;
  zoomFloor.value = rootPath;
  zoomRoot.value = rootPath;
}

async function zoomIn(path: string): Promise<void> {
  await setExpanded(path, true);
  zoomRoot.value = path;
}

function zoomOut(): void {
  if (!zoomRoot.value || zoomRoot.value === zoomFloor.value) return;
  zoomRoot.value = parentPath(zoomRoot.value);
}

function canZoomOut(): boolean {
  return !!zoomRoot.value && zoomRoot.value !== zoomFloor.value;
}

function canZoomInSelected(): boolean {
  return selectedIsDir.value && !!selectedPath.value && selectedPath.value !== zoomRoot.value;
}

async function zoomInSelected(): Promise<void> {
  if (!canZoomInSelected() || !selectedPath.value) return;
  await zoomIn(selectedPath.value);
}

/**
 * One module-level store per window/webview context (each Tauri window is
 * its own JS runtime, so this singleton never leaks across volume windows).
 * It's the single source of truth for loaded directory listings so the tree
 * list and the treemap can share expansion/selection/hover state and stay
 * in sync without prop-drilling.
 */
export function useFsTree() {
  return {
    expanded,
    selectedPath,
    selectedIsDir,
    hoveredPath,
    zoomRoot,
    scanTotalBytes,
    scanScannedBytes,
    scanScannedFiles,
    isScanning,
    peek,
    ensureChildren,
    startScan,
    isExpanded,
    setExpanded,
    toggleExpanded,
    select,
    hover,
    initZoom,
    zoomIn,
    zoomOut,
    canZoomOut,
    canZoomInSelected,
    zoomInSelected,
  };
}
