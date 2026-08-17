<script setup lang="ts">
import { computed, watch } from "vue";
import TreeNode from "./TreeNode.vue";
import ProgressBar from "./ProgressBar.vue";
import { useFsTree } from "../composables/fsTree";

const props = defineProps<{ rootPath: string }>();

const tree = useFsTree();

watch(() => props.rootPath, (path) => tree.ensureChildren(path), { immediate: true });

const root = computed(() => tree.peek(props.rootPath));

const rootName = computed(() => {
  const trimmed = props.rootPath.endsWith("/") && props.rootPath !== "/"
    ? props.rootPath.slice(0, -1)
    : props.rootPath;

  return trimmed.slice(trimmed.lastIndexOf("/") + 1) || trimmed;
});

const isAtRoot = computed(() => (tree.zoomRoot.value ?? props.rootPath) === props.rootPath);
const isRootSelected = computed(() => tree.selectedPath.value === props.rootPath);

function goToRoot() {
  tree.select(props.rootPath, true);
  tree.zoomToRoot();
}
</script>

<template>
  <div class="file-tree">
    <div
      class="root-row"
      :class="{ 'is-selected': isRootSelected, 'is-at-root': isAtRoot }"
      :title="isAtRoot ? rootPath : `Back to ${rootPath}`"
      @click="goToRoot"
    >
      <span class="icon">💾</span>
      <span class="name">{{ rootName }}</span>
    </div>

    <ProgressBar v-if="root.isLoading" label="Loading files and folders..." />
    <p v-else-if="root.error" class="status error">{{ root.error }}</p>
    <p v-else-if="root.children && root.children.length === 0" class="status">Empty</p>
    <ul v-else class="tree-root">
      <TreeNode
        v-for="entry in root.children ?? []"
        :key="entry.path"
        :entry="entry"
        :depth="0"
      />
    </ul>
  </div>
</template>

<style scoped>
.file-tree {
  border: 1px solid rgba(128, 128, 128, 0.3);
  border-radius: 10px;
  padding: 0.5rem;
  height: 100%;
  box-sizing: border-box;
  overflow-y: auto;
  text-align: left;
}

.root-row {
  display: flex;
  align-items: center;
  gap: 0.4rem;
  padding: 0.3rem 0.5rem;
  border-radius: 6px;
  cursor: pointer;
  font-weight: 600;
  white-space: nowrap;
  margin-bottom: 0.3rem;
}

.root-row:hover {
  background-color: rgba(57, 108, 216, 0.1);
}

.root-row.is-selected {
  background-color: rgba(57, 108, 216, 0.22);
}

.root-row .name {
  overflow: hidden;
  text-overflow: ellipsis;
}

.root-row:not(.is-at-root)::after {
  content: "← back";
  margin-left: auto;
  font-size: 0.7rem;
  font-weight: 400;
  color: #888;
}

.tree-root {
  margin: 0;
  padding: 0;
}

.status {
  font-size: 0.85rem;
  color: #888;
  padding: 0.4rem;
}

.status.error {
  color: #d33;
}
</style>
