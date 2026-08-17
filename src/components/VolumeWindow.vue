<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

import FileTree from "./FileTree.vue";
import TreemapPanel from "./TreemapPanel.vue";
import TreemapLegend from "./TreemapLegend.vue";
import ProgressBar from "./ProgressBar.vue";

import { useFsTree } from "../composables/fsTree";
import { formatBytes } from "../utils/format";

const props = defineProps<{
  rootPath: string;
}>();

const tree = useFsTree();

const scanPercent = computed(() => {
  if (tree.scanTotalBytes.value <= 0) {
    return 0;
  }

  return Math.min(
    100,
    Math.round(
      (tree.scanScannedBytes.value /
        tree.scanTotalBytes.value) *
        100,
    ),
  );
});

const scanningLabel = computed(() => {
  if (!tree.isScanning.value) {
    return "";
  }

  const files = `${tree.scanScannedFiles.value.toLocaleString()} files`;
  const scanned = formatBytes(tree.scanScannedBytes.value);

  if (tree.scanTotalBytes.value <= 0) {
    return `Scanning ${files} (${scanned})`;
  }

  const total = formatBytes(tree.scanTotalBytes.value);
  return `Scanning ${files} · ${scanned} of ${total}`;
});

const statusPath = computed(() => props.rootPath);

const showLegend = computed(() => {
  return !tree.isScanning.value;
});

const isClosing = ref(false);

async function closeVolumeWindow() {
  if (isClosing.value) return;
  isClosing.value = true;

  try {
    await invoke("close_volume_window");
  } catch (err) {
    console.error("Failed to close volume window:", err);
    isClosing.value = false;
  }
}

// -----------------------------------------------------------------------------
// Tree/treemap split resizing
// -----------------------------------------------------------------------------

const treeWidth = ref(320);
const isResizing = ref(false);
const bodyEl = ref<HTMLElement | null>(null);

const TREE_MIN_WIDTH = 180;
const TREE_RIGHT_RESERVED = 240; // leave room for the treemap + legend

function startResize(event: MouseEvent) {
  isResizing.value = true;
  event.preventDefault();
  window.addEventListener("mousemove", handleResize);
  window.addEventListener("mouseup", stopResize);
}

function handleResize(event: MouseEvent) {
  if (!isResizing.value || !bodyEl.value) return;

  const rect = bodyEl.value.getBoundingClientRect();
  const maxWidth = Math.max(TREE_MIN_WIDTH, rect.width - TREE_RIGHT_RESERVED);
  const newWidth = event.clientX - rect.left;

  treeWidth.value = Math.min(Math.max(newWidth, TREE_MIN_WIDTH), maxWidth);
}

function stopResize() {
  isResizing.value = false;
  window.removeEventListener("mousemove", handleResize);
  window.removeEventListener("mouseup", stopResize);
}

onBeforeUnmount(() => {
  window.removeEventListener("mousemove", handleResize);
  window.removeEventListener("mouseup", stopResize);
});

onMounted(() => {
  tree.initZoom(props.rootPath);

  void tree.startScan(props.rootPath);
});
</script>

<template>
  <div class="volume-window">
    <header class="toolbar">
      <button
        type="button"
        class="close-button"
        :disabled="isClosing"
        title="Close and return to the disk list"
        @click="closeVolumeWindow"
      >
        Close
      </button>
    </header>

    <div
      v-if="tree.isScanning.value"
      class="scan-banner"
    >
      <ProgressBar
        :label="scanningLabel"
        :percentage="scanPercent"
      />
    </div>

    <div ref="bodyEl" class="body">
      <FileTree
        :root-path="rootPath"
        class="pane tree"
        :style="{ width: treeWidth + 'px' }"
      />

      <div
        class="divider"
        :class="{ 'is-resizing': isResizing }"
        @mousedown="startResize"
      ></div>

      <TreemapPanel
        :root-path="rootPath"
        class="pane treemap"
      />

      <TreemapLegend
        v-if="showLegend"
        :root-path="rootPath"
      />
    </div>

    <footer class="statusbar">
      <span class="statusbar-path">
        {{ statusPath }}
      </span>

      <span
        v-if="tree.isScanning.value"
        class="statusbar-scan"
      >
        {{ tree.scanScannedFiles.value.toLocaleString() }} files
        ·
        {{ formatBytes(tree.scanScannedBytes.value) }}
      </span>
    </footer>
  </div>
</template>

<style scoped>
.volume-window {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-width: 0;
  min-height: 0;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex: 0 0 auto;
  padding: 8px 12px;
  border-bottom: 1px solid rgba(128, 128, 128, 0.2);
}

.close-button {
  font-size: 0.8rem;
  padding: 0.35em 0.9em;
}

.scan-banner {
  flex: 0 0 auto;
  padding: 8px 12px;
}

.body {
  display: flex;
  flex: 1 1 auto;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

.pane {
  min-width: 0;
  min-height: 0;
}

.tree {
  flex: 0 0 auto;
  min-width: 180px;
  max-width: 70%;
  overflow: auto;
}

.divider {
  flex: 0 0 6px;
  cursor: col-resize;
  position: relative;
}

.divider::after {
  content: "";
  position: absolute;
  top: 0;
  bottom: 0;
  left: 2px;
  width: 2px;
  background: rgba(128, 128, 128, 0.25);
  transition: background 0.15s;
}

.divider:hover::after,
.divider.is-resizing::after {
  background: rgba(57, 108, 216, 0.6);
}

.treemap {
  flex: 1 1 auto;
  overflow: hidden;
}

.statusbar {
  display: flex;
  align-items: center;
  gap: 16px;
  flex: 0 0 28px;
  padding: 0 10px;
  overflow: hidden;
  white-space: nowrap;
}

.statusbar-path {
  flex: 1 1 auto;
  overflow: hidden;
  text-overflow: ellipsis;
}

.statusbar-scan {
  flex: 0 0 auto;
  opacity: 0.75;
}
</style>
