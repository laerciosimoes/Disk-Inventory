<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import type { DiskInfo } from "../types";
import { formatBytes } from "../utils/format";

const disks = ref<DiskInfo[]>([]);
const isLoading = ref(false);
const isOpening = ref(false);
const errorMessage = ref<string | null>(null);

async function loadDisks() {
  isLoading.value = true;
  errorMessage.value = null;
  try {
    disks.value = await invoke<DiskInfo[]>("list_disks");
  } catch (err) {
    errorMessage.value = String(err);
  } finally {
    isLoading.value = false;
  }
}

async function selectDisk(mountPoint: string) {
  if (isOpening.value) return;
  isOpening.value = true;
  try {
    await invoke("open_volume_window", { mountPoint });
  } catch (err) {
    errorMessage.value = String(err);
    isOpening.value = false;
  }
}

onMounted(loadDisks);
</script>

<template>
  <main class="container">
    <header class="toolbar">
      <h1>Disk Inventory</h1>
      <button @click="loadDisks" :disabled="isLoading">
        {{ isLoading ? "Refreshing..." : "Refresh" }}
      </button>
    </header>

    <p v-if="errorMessage" class="error">{{ errorMessage }}</p>

    <ul
      class="disk-list"
      role="listbox"
      aria-label="Available drives"
      :class="{ opening: isOpening }"
    >
      <li
        v-for="disk in disks"
        :key="disk.mountPoint"
        role="option"
        class="disk-item"
        @click="selectDisk(disk.mountPoint)"
      >
        <div class="disk-row">
          <span class="disk-name">{{ disk.name || disk.mountPoint }}</span>
          <span class="disk-tag" v-if="disk.isRemovable">Removable</span>
          <span class="disk-tag" v-if="disk.isReadOnly">Read-only</span>
        </div>
        <div class="disk-mount">{{ disk.mountPoint }}</div>
        <div class="usage-bar">
          <div
            class="usage-fill"
            :style="{ width: Math.min(disk.usedPercent, 100) + '%' }"
          ></div>
        </div>
        <div class="disk-details">
          <span>{{ formatBytes(disk.usedBytes) }} used of {{ formatBytes(disk.totalBytes) }}</span>
          <span>{{ disk.usedPercent.toFixed(1) }}%</span>
          <span>{{ disk.fileSystem }}</span>
        </div>
      </li>
    </ul>

    <p v-if="!isLoading && disks.length === 0 && !errorMessage" class="empty">
      No drives found.
    </p>
  </main>
</template>

<style scoped>
.container {
  margin: 0 auto;
  padding: 2rem;
  max-width: 760px;
  text-align: left;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 1.5rem;
}

.toolbar h1 {
  margin: 0;
  font-size: 1.4rem;
}

.error {
  color: #d33;
}

.empty {
  color: #888;
}

.disk-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.disk-list.opening {
  opacity: 0.6;
  pointer-events: none;
}

.disk-item {
  border: 1px solid rgba(128, 128, 128, 0.3);
  border-radius: 10px;
  padding: 0.9rem 1rem;
  cursor: pointer;
  transition: border-color 0.15s, background-color 0.15s;
}

.disk-item:hover {
  border-color: #396cd8;
}

.disk-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.disk-name {
  font-weight: 600;
}

.disk-tag {
  font-size: 0.7rem;
  padding: 0.1rem 0.45rem;
  border-radius: 999px;
  background: rgba(128, 128, 128, 0.2);
}

.disk-mount {
  font-size: 0.85rem;
  color: #888;
  margin-top: 0.15rem;
}

.usage-bar {
  margin-top: 0.6rem;
  height: 8px;
  border-radius: 4px;
  background: rgba(128, 128, 128, 0.2);
  overflow: hidden;
}

.usage-fill {
  height: 100%;
  background: #396cd8;
}

.disk-details {
  display: flex;
  gap: 0.9rem;
  margin-top: 0.5rem;
  font-size: 0.8rem;
  color: #888;
}
</style>
