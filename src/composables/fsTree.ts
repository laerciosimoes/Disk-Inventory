import { reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { FsEntry } from "../types";

interface NodeState {
  children: FsEntry[] | null;
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
    node.children = await invoke<FsEntry[]>("list_directory", { path });
  } catch (err) {
    node.error = String(err);
    node.children = [];
  } finally {
    node.isLoading = false;
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
    peek,
    ensureChildren,
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
