<script setup lang="ts">
import { computed, watch } from "vue";
import TreeNode from "./TreeNode.vue";
import ProgressBar from "./ProgressBar.vue";
import { useFsTree } from "../composables/fsTree";

const props = defineProps<{ rootPath: string }>();

const tree = useFsTree();

watch(() => props.rootPath, (path) => tree.ensureChildren(path), { immediate: true });

const root = computed(() => tree.peek(props.rootPath));
</script>

<template>
  <div class="file-tree">
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
