<script setup lang="ts">
withDefaults(defineProps<{ label?: string }>(), { label: "" });
</script>

<template>
  <div class="progress">
    <p v-if="label" class="progress-label">{{ label }}</p>
    <div class="progress-track" role="progressbar" aria-label="Loading">
      <div class="progress-fill progress-fill--a"></div>
      <div class="progress-fill progress-fill--b"></div>
    </div>
  </div>
</template>

<style scoped>
.progress {
  padding: 0.6rem;
}

.progress-label {
  margin: 0 0 0.5rem;
  font-size: 0.85rem;
  color: #888;
}

.progress-track {
  position: relative;
  height: 4px;
  border-radius: 999px;
  background: rgba(128, 128, 128, 0.2);
  overflow: hidden;
}

.progress-fill {
  position: absolute;
  top: 0;
  left: 0;
  height: 100%;
  border-radius: 999px;
  background: #396cd8;
  animation-duration: 1.6s;
  animation-timing-function: cubic-bezier(0.4, 0, 0.2, 1);
  animation-iteration-count: infinite;
}

.progress-fill--a {
  animation-name: progress-slide-a;
}

.progress-fill--b {
  animation-name: progress-slide-b;
  animation-delay: 0.8s;
}

@keyframes progress-slide-a {
  0% {
    left: -35%;
    width: 35%;
  }
  60% {
    left: 100%;
    width: 60%;
  }
  100% {
    left: 100%;
    width: 60%;
  }
}

@keyframes progress-slide-b {
  0% {
    left: -70%;
    width: 50%;
  }
  60% {
    left: 107%;
    width: 25%;
  }
  100% {
    left: 107%;
    width: 25%;
  }
}
</style>
