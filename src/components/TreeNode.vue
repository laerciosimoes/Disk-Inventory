<script setup lang="ts">
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { FsEntry } from "../types";
import { formatBytes } from "../utils/format";

const props = defineProps<{ entry: FsEntry; depth: number }>();

const isExpanded = ref(false);
const isLoading = ref(false);
const children = ref<FsEntry[] | null>(null);
const errorMessage = ref<string | null>(null);

async function toggle() {
  if (!props.entry.isDir) return;
  isExpanded.value = !isExpanded.value;
  if (isExpanded.value && children.value === null) {
    isLoading.value = true;
    errorMessage.value = null;
    try {
      children.value = await invoke<FsEntry[]>("list_directory", {
        path: props.entry.path,
      });
    } catch (err) {
      errorMessage.value = String(err);
      children.value = [];
    } finally {
      isLoading.value = false;
    }
  }
}
</script>

<template>
  <li class="node">
    <div
      class="node-row"
      :class="{ 'is-dir': entry.isDir }"
      :style="{ paddingLeft: depth * 1.1 + 'rem' }"
      @click="toggle"
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
        v-if="isLoading"
        class="status"
        :style="{ paddingLeft: (depth + 1) * 1.1 + 'rem' }"
      >
        Loading...
      </li>
      <li
        v-else-if="errorMessage"
        class="status error"
        :style="{ paddingLeft: (depth + 1) * 1.1 + 'rem' }"
      >
        {{ errorMessage }}
      </li>
      <li
        v-else-if="children && children.length === 0"
        class="status"
        :style="{ paddingLeft: (depth + 1) * 1.1 + 'rem' }"
      >
        Empty
      </li>
      <TreeNode
        v-else
        v-for="child in children"
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

.node-row:hover {
  background-color: rgba(57, 108, 216, 0.1);
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
