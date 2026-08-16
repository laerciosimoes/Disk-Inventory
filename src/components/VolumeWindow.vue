<script setup lang="ts">
import { computed, onMounted } from "vue";

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

  return `Scanning ${tree.scanScannedFiles.value.toLocaleString()} files`;
});

const statusPath = computed(() => props.rootPath);

const showLegend = computed(() => {
  return !tree.isScanning.value;
});

onMounted(() => {
  tree.initZoom(props.rootPath);

  void tree.startScan(props.rootPath);
});
</script>

<template>
  <div class="volume-window">
    <div
      v-if="tree.isScanning.value"
      class="scan-banner"
    >
      <ProgressBar
        :label="scanningLabel"
        :percentage="scanPercent"
      />
    </div>

    <div class="body">
      <FileTree
        :root-path="rootPath"
        class="pane tree"
      />

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
  flex: 0 0 320px;
  overflow: auto;
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