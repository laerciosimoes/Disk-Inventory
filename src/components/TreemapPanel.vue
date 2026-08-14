<script setup lang="ts">
import { computed, watch } from "vue";
import { useFsTree } from "../composables/fsTree";
import { squarify, type TreemapRect } from "../utils/treemap";
import TreemapNode from "./TreemapNode.vue";
import ProgressBar from "./ProgressBar.vue";

const props = defineProps<{ rootPath: string }>();

const tree = useFsTree();

watch(
  () => props.rootPath,
  (path) => {
    tree.initZoom(path);
    tree.ensureChildren(path);
  },
  { immediate: true }
);

const zoomRoot = computed(() => tree.zoomRoot.value ?? props.rootPath);
const zoomNode = computed(() => tree.peek(zoomRoot.value));

const rects = computed<TreemapRect[]>(() => {
  const children = zoomNode.value.children;
  if (!children) return [];
  return squarify(
    children.map((c) => ({ key: c.path, value: c.sizeBytes })),
    0,
    0,
    100,
    100
  );
});

function childEntry(key: string) {
  return zoomNode.value.children?.find((c) => c.path === key);
}
</script>

<template>
  <div class="treemap" @mouseleave="tree.hover(null)">
    <ProgressBar v-if="zoomNode.isLoading" label="Computing sizes..." />
    <p v-else-if="zoomNode.error" class="status error">{{ zoomNode.error }}</p>
    <p v-else-if="zoomNode.children && zoomNode.children.length === 0" class="status">
      Empty
    </p>
    <div v-else class="treemap-viewport">
      <TreemapNode
        v-for="r in rects"
        :key="r.key"
        :entry="childEntry(r.key)!"
        :rect="r"
        :depth="0"
        :root-path="rootPath"
        @zoom="(p) => tree.zoomIn(p)"
      />
    </div>
  </div>
</template>

<style scoped>
.treemap {
  border: 1px solid rgba(128, 128, 128, 0.3);
  border-radius: 10px;
  height: 100%;
  box-sizing: border-box;
  overflow: hidden;
  position: relative;
}

.treemap-viewport {
  position: relative;
  width: 100%;
  height: 100%;
}

.status {
  font-size: 0.85rem;
  color: #888;
  padding: 0.6rem;
}

.status.error {
  color: #d33;
}
</style>
