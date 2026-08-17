<script setup lang="ts">
import { computed } from "vue";
import { useFsTree } from "../composables/fsTree";
import { formatBytes } from "../utils/format";
import { swatchForEntry } from "../utils/treemapColor";

const props = defineProps<{ rootPath: string }>();

const tree = useFsTree();

const zoomRoot = computed(() => tree.zoomRoot.value ?? props.rootPath);
const children = computed(() => tree.peek(zoomRoot.value).children ?? []);
const total = computed(() => children.value.reduce((sum, c) => sum + c.sizeBytes, 0));

function percent(bytes: number): string {
  if (total.value <= 0) return "0%";
  return ((bytes / total.value) * 100).toFixed(1) + "%";
}
</script>

<template>
  <aside class="legend">
    <h2>Contents</h2>
    <ul>
      <li
        v-for="entry in children"
        :key="entry.path"
        :class="{ 'is-hovered': tree.hoveredPath.value === entry.path }"
        @mouseenter="tree.hover(entry.path)"
        @mouseleave="tree.hover(null)"
        @click="tree.select(entry.path, entry.isDir)"
      >
        <span class="swatch" :style="{ background: swatchForEntry(entry) }" />
        <span class="name">{{ entry.name }}</span>
        <span class="percent">{{ percent(entry.sizeBytes) }}</span>
        <span class="size">{{ formatBytes(entry.sizeBytes) }}</span>
      </li>
    </ul>
  </aside>
</template>

<style scoped>
.legend {
  border: 1px solid rgba(128, 128, 128, 0.3);
  border-radius: 10px;
  padding: 0.6rem;
  box-sizing: border-box;
  height: 100%;
  overflow-y: auto;
  width: 220px;
  flex: none;
}

.legend h2 {
  margin: 0.2rem 0 0.6rem;
  font-size: 0.8rem;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: #888;
}

.legend ul {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
}

.legend li {
  display: flex;
  align-items: center;
  gap: 0.45rem;
  padding: 0.3rem 0.4rem;
  border-radius: 6px;
  cursor: pointer;
  font-size: 0.78rem;
}

.legend li:hover,
.legend li.is-hovered {
  background-color: rgba(57, 108, 216, 0.1);
}

.swatch {
  width: 0.65rem;
  height: 0.65rem;
  border-radius: 3px;
  flex: none;
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.percent {
  color: #888;
  flex: none;
}

.size {
  color: #888;
  flex: none;
  min-width: 3.6rem;
  text-align: right;
}
</style>
