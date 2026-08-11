<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import DiskList from "./components/DiskList.vue";
import VolumeWindow from "./components/VolumeWindow.vue";

const isResolved = ref(false);
const mountPoint = ref<string | null>(null);

onMounted(async () => {
  try {
    mountPoint.value = await invoke<string | null>("get_window_mount_point");
  } catch {
    mountPoint.value = null;
  } finally {
    isResolved.value = true;
  }
});
</script>

<template>
  <template v-if="isResolved">
    <VolumeWindow v-if="mountPoint" :root-path="mountPoint" />
    <DiskList v-else />
  </template>
</template>

<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;

  color: #0f0f0f;
  background-color: #f6f6f6;

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

body {
  margin: 0;
}

button {
  border-radius: 8px;
  border: 1px solid transparent;
  padding: 0.5em 1em;
  font-size: 0.9em;
  font-weight: 500;
  font-family: inherit;
  color: #0f0f0f;
  background-color: #ffffff;
  cursor: pointer;
  transition: border-color 0.25s;
  box-shadow: 0 2px 2px rgba(0, 0, 0, 0.2);
  outline: none;
}

button:hover:not(:disabled) {
  border-color: #396cd8;
}

button:disabled {
  opacity: 0.6;
  cursor: default;
}

@media (prefers-color-scheme: dark) {
  :root {
    color: #f6f6f6;
    background-color: #2f2f2f;
  }

  button {
    color: #ffffff;
    background-color: #0f0f0f98;
  }
}
</style>
