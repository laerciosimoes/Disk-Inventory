<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import FileTree from "./FileTree.vue";
import TreemapPanel from "./TreemapPanel.vue";
import TreemapLegend from "./TreemapLegend.vue";
import ProgressBar from "./ProgressBar.vue";
import { useFsTree } from "../composables/fsTree";

const props = defineProps<{ rootPath: string }>();

const tree = useFsTree();
const showLegend = ref(true);

const statusPath = computed(() => tree.hoveredPath.value ?? tree.selectedPath.value ?? props.rootPath);

// Drive the root scan from here (not just from FileTree/TreemapPanel) so
// progress starts the instant this window mounts, rather than depending on
// a child pane having rendered first.
watch(() => props.rootPath, (path) => tree.startScan(path), { immediate: true });

const scanPercent = computed(() => {
  const total = tree.scanTotalBytes.value;
  if (total <= 0) return 0;
  return Math.min(100, (tree.scanScannedBytes.value / total) * 100);
});

const scanningLabel = computed(
  () =>
    `Scanning ${props.rootPath} — this walks every folder to compute sizes, ` +
    `so large or system volumes can take a while (${scanPercent.value.toFixed(0)}%)`
);

function closeWindow() {
  invoke("close_current_window");
}
</script>

<template>
  <main class="container">
    <header class="toolbar">
      <h1>{{ rootPath }}</h1>
      <div class="toolbar-actions">
        <button
          title="Zoom in on the selected folder"
          :disabled="!tree.canZoomInSelected()"
          @click="tree.zoomInSelected"
        >
          Zoom In
        </button>
        <button
          title="Zoom out to the parent folder"
          :disabled="!tree.canZoomOut()"
          @click="tree.zoomOut"
        >
          Zoom Out
        </button>
        <button @click="showLegend = !showLegend">
          {{ showLegend ? "Hide" : "Show" }} File Kind Statistics
        </button>
        <button @click="closeWindow">Close</button>
      </div>
    </header>

    <div v-if="tree.isScanning.value" class="scan-banner">
      <ProgressBar :label="scanningLabel" :percentage="scanPercent" />
    </div>

    <div class="body">
      <FileTree :root-path="rootPath" class="pane tree" />
      <TreemapPanel :root-path="rootPath" class="pane treemap" />
      <TreemapLegend v-if="showLegend" :root-path="rootPath" />
    </div>

    <footer class="statusbar">
      <span class="statusbar-path">{{ statusPath }}</span>
    </footer>
  </main>
</template>

<style scoped>
.container {
  margin: 0 auto;
  padding: 1.25rem 1.5rem;
  max-width: 100%;
  height: 100vh;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  text-align: left;
  gap: 0.75rem;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
}

.toolbar h1 {
  margin: 0;
  font-size: 1.2rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.toolbar-actions {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex: none;
}

.body {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: 0.75rem;
}

.scan-banner {
  flex: none;
  border: 1px solid rgba(128, 128, 128, 0.3);
  border-radius: 10px;
}

.scan-banner :deep(.progress) {
  padding: 0.4rem 0.6rem;
}

.pane {
  min-width: 0;
}

.tree {
  flex: 0 0 40%;
}

.treemap {
  flex: 1;
}

.statusbar {
  flex: none;
  font-size: 0.78rem;
  color: #888;
  padding: 0.3rem 0.2rem 0;
  border-top: 1px solid rgba(128, 128, 128, 0.3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
