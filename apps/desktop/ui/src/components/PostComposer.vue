<script setup lang="ts">
import { ref } from "vue";
import { api } from "../api";
import type { OwnInfo } from "../types";

defineProps<{ own: OwnInfo | null }>();
const emit = defineEmits<{ posted: [] }>();

const text = ref("");
const busy = ref(false);
const error = ref("");

async function submit() {
  const t = text.value.trim();
  if (!t || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await api.publishPost(t);
    text.value = "";
    emit("posted");
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="composer">
    <textarea
      v-model="text"
      rows="3"
      maxlength="1000"
      placeholder="分享你的想法…（端到端加密广播给所有好友）"
      @keydown.ctrl.enter="submit"
    ></textarea>
    <div class="row">
      <span class="hint">Ctrl+Enter 发送 · 上限 1000 字符</span>
      <span v-if="error" class="error">{{ error }}</span>
      <button class="primary" :disabled="busy || !text.trim()" @click="submit">
        {{ busy ? "发送中…" : "发布" }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.composer {
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px;
  margin-bottom: 16px;
}
textarea {
  border: none;
  background: transparent;
  padding: 4px;
}
.row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 8px;
}
.hint {
  color: var(--text-dim);
  font-size: 12px;
  flex: 1;
}
.error {
  color: var(--danger);
  font-size: 12px;
}
</style>
