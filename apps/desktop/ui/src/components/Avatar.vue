<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  src?: string | null;
  name?: string | null;
  size?: number;
}>();

const initial = computed(() => {
  const n = (props.name || "?").trim();
  return n ? n[0].toUpperCase() : "?";
});
</script>

<template>
  <span
    class="avatar"
    :style="{ width: (size || 36) + 'px', height: (size || 36) + 'px', fontSize: ((size || 36) * 0.42) + 'px' }"
  >
    <img v-if="src" :src="src" alt="" />
    <span v-else class="placeholder">{{ initial }}</span>
  </span>
</template>

<style scoped>
.avatar {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 50%;
  overflow: hidden;
  background: var(--bg-3);
  border: 1px solid var(--border);
  flex-shrink: 0;
  vertical-align: middle;
}
.avatar img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.placeholder {
  color: var(--text-dim);
  font-weight: 700;
}
</style>
