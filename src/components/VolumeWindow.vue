<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import FileTree from "./FileTree.vue";

defineProps<{ rootPath: string }>();

function closeWindow() {
  invoke("close_current_window");
}
</script>

<template>
  <main class="container">
    <header class="toolbar">
      <h1>{{ rootPath }}</h1>
      <button @click="closeWindow">Close</button>
    </header>

    <FileTree :root-path="rootPath" class="tree" />
  </main>
</template>

<style scoped>
.container {
  margin: 0 auto;
  padding: 2rem;
  max-width: 100%;
  height: 100vh;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  text-align: left;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  margin-bottom: 1.5rem;
}

.toolbar h1 {
  margin: 0;
  font-size: 1.2rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tree {
  flex: 1;
  max-height: none;
}
</style>
