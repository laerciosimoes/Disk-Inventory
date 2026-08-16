<script setup lang="ts">
import { computed } from "vue";
import type { TreeEntry } from "../types";
import { useFsTree } from "../composables/fsTree";
import { squarify, type TreemapRect } from "../utils/treemap";
import { backgroundForEntry } from "../utils/treemapColor";

const props = defineProps<{
  entry: TreeEntry;
  rect: TreemapRect;
  depth: number;
  rootPath: string;
}>();

const emit = defineEmits<{ zoom: [path: string] }>();

const tree = useFsTree();

const node = computed(() => tree.peek(props.entry.path));
const isExpanded = computed(() => tree.isExpanded(props.entry.path));
const isHovered = computed(() => tree.hoveredPath.value === props.entry.path);
const isSelected = computed(() => tree.selectedPath.value === props.entry.path);

const childRects = computed<TreemapRect[]>(() => {
  const children = node.value.children;
  if (!props.entry.isDir || !isExpanded.value || !children) return [];
  return squarify(
    children.map((c) => ({ key: c.path, value: c.sizeBytes })),
    0,
    0,
    100,
    100
  );
});

function childEntry(key: string): TreeEntry | undefined {
  return node.value.children?.find((c) => c.path === key);
}

const positionStyle = computed(() => ({
  left: props.rect.x + "%",
  top: props.rect.y + "%",
  width: props.rect.width + "%",
  height: props.rect.height + "%",
  background: backgroundForEntry(props.entry.path, props.rootPath, props.depth),
}));

const showLabel = computed(() => props.rect.width > 6 && props.rect.height > 4);

function onClick(event: MouseEvent) {
  event.stopPropagation();
  tree.select(props.entry.path, props.entry.isDir);
}

async function onDblClick(event: MouseEvent) {
  event.stopPropagation();
  if (!props.entry.isDir) return;
  tree.select(props.entry.path, true);
  await tree.setExpanded(props.entry.path, true);
  emit("zoom", props.entry.path);
}
</script>

<template>
  <div
    class="tm-node"
    :class="{ 'is-hovered': isHovered, 'is-selected': isSelected }"
    :style="positionStyle"
    :title="entry.path"
    @mouseenter.stop="tree.hover(entry.path)"
    @mouseleave.stop="tree.hover(null)"
    @click="onClick"
    @dblclick="onDblClick"
  >
    <span v-if="showLabel" class="tm-label">{{ entry.name }}</span>

    <TreemapNode
      v-for="cr in childRects"
      :key="cr.key"
      :entry="childEntry(cr.key)!"
      :rect="cr"
      :depth="depth + 1"
      :root-path="rootPath"
      @zoom="(p) => emit('zoom', p)"
    />
  </div>
</template>

<style scoped>
.tm-node {
  position: absolute;
  box-sizing: border-box;
  border: 1px solid rgba(0, 0, 0, 0.35);
  overflow: hidden;
  cursor: pointer;
}

.tm-node.is-hovered,
.tm-node.is-selected {
  outline: 2px solid #ffd54a;
  outline-offset: -2px;
  z-index: 1;
}

.tm-label {
  position: absolute;
  top: 2px;
  left: 4px;
  right: 4px;
  font-size: 0.7rem;
  color: rgba(255, 255, 255, 0.92);
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  pointer-events: none;
}
</style>
