<script setup lang="ts">
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { FsEntry } from "../types";
import TreeNode from "./TreeNode.vue";

const props = defineProps<{ rootPath: string }>();

const rootEntries = ref<FsEntry[]>([]);
const isLoading = ref(false);
const errorMessage = ref<string | null>(null);

async function loadRoot() {
  isLoading.value = true;
  errorMessage.value = null;
  try {
    rootEntries.value = await invoke<FsEntry[]>("list_directory", {
      path: props.rootPath,
    });
  } catch (err) {
    errorMessage.value = String(err);
    rootEntries.value = [];
  } finally {
    isLoading.value = false;
  }
}

watch(() => props.rootPath, loadRoot, { immediate: true });
</script>

<template>
  <div class="file-tree">
    <p v-if="isLoading" class="status">Loading...</p>
    <p v-else-if="errorMessage" class="status error">{{ errorMessage }}</p>
    <p v-else-if="rootEntries.length === 0" class="status">Empty</p>
    <ul v-else class="tree-root">
      <TreeNode
        v-for="entry in rootEntries"
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
