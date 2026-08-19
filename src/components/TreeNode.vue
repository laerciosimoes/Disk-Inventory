<script setup lang="ts">
import { computed } from "vue";
import type { TreeEntry } from "../types";
import { formatBytes } from "../utils/format";
import { useFsTree } from "../composables/fsTree";
import { showEntryContextMenu } from "../utils/contextMenu";

const props = defineProps<{ entry: TreeEntry; depth: number }>();

const tree = useFsTree();

const node = computed(() => tree.peek(props.entry.path));
const isExpanded = computed(() => tree.isExpanded(props.entry.path));
const isSelected = computed(() => tree.selectedPath.value === props.entry.path);
const isHovered = computed(() => tree.hoveredPath.value === props.entry.path);

async function toggle() {
  tree.select(props.entry.path, props.entry.isDir);
  if (!props.entry.isDir) return;
  await tree.toggleExpanded(props.entry.path);
}

function onContextMenu(event: MouseEvent) {
  void showEntryContextMenu(props.entry, event);
}
</script>

<template>
  <li class="node">
    <div
      class="node-row"
      :class="{ 'is-dir': entry.isDir, 'is-selected': isSelected, 'is-hovered': isHovered }"
      :style="{ paddingLeft: depth * 1.1 + 'rem' }"
      @click="toggle"
      @contextmenu="onContextMenu"
      @mouseenter="tree.hover(entry.path)"
      @mouseleave="tree.hover(null)"
    >
      <span class="disclosure">{{
        entry.isDir ? (isExpanded ? "▾" : "▸") : ""
      }}</span>
      <span class="icon">{{ entry.isDir ? "📁" : "📄" }}</span>
      <span class="name">{{ entry.name }}</span>
      <span class="size">{{ formatBytes(entry.sizeBytes) }}</span>
    </div>

    <ul v-if="entry.isDir && isExpanded" class="children">
      <li
        v-if="node.isLoading"
        class="status"
        :style="{ paddingLeft: (depth + 1) * 1.1 + 'rem' }"
      >
        Loading...
      </li>
      <li
        v-else-if="node.error"
        class="status error"
        :style="{ paddingLeft: (depth + 1) * 1.1 + 'rem' }"
      >
        {{ node.error }}
      </li>
      <li
        v-else-if="node.children && node.children.length === 0"
        class="status"
        :style="{ paddingLeft: (depth + 1) * 1.1 + 'rem' }"
      >
        Empty
      </li>
      <TreeNode
        v-else
        v-for="child in node.children"
        :key="child.path"
        :entry="child"
        :depth="depth + 1"
      />
    </ul>
  </li>
</template>

<style scoped>
.node {
  list-style: none;
}

.node-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.25rem 0.5rem;
  border-radius: 6px;
  cursor: default;
  white-space: nowrap;
}

.node-row.is-dir {
  cursor: pointer;
}

.node-row:hover,
.node-row.is-hovered {
  background-color: rgba(57, 108, 216, 0.1);
}

.node-row.is-selected {
  background-color: rgba(57, 108, 216, 0.22);
}

.disclosure {
  width: 0.9rem;
  display: inline-block;
  text-align: center;
  color: #888;
  font-size: 0.75rem;
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
}

.size {
  margin-left: auto;
  padding-left: 1rem;
  color: #888;
  font-size: 0.8rem;
}

.children {
  margin: 0;
  padding: 0;
}

.status {
  font-size: 0.8rem;
  color: #888;
  padding: 0.2rem 0.5rem;
}

.status.error {
  color: #d33;
}
</style>
