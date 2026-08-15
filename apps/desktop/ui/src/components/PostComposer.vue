<script setup lang="ts">
import { ref } from "vue";
import { api } from "../api";
import { t } from "../i18n";
import type { OwnInfo } from "../types";

defineProps<{ own: OwnInfo | null }>();
const emit = defineEmits<{ posted: [] }>();

const text = ref("");
const busy = ref(false);
const error = ref("");
const uploading = ref(false);
const isPublic = ref(false);
const mediaError = ref("");
const fileInput = ref<HTMLInputElement | null>(null);

function insertImageUrl() {
  const url = prompt("输入图片 URL（http/https）：");
  if (!url) return;
  const md = `![图片](${url.trim()})`;
  text.value = text.value ? `${text.value}
${md}` : md;
}

async function onFileSelected(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file || uploading.value) return;
  if (file.size > 10 * 1024 * 1024) {
    mediaError.value = "文件不能超过 10MB";
    return;
  }
  uploading.value = true;
  mediaError.value = "";
  try {
    const dataUrl = await readFileAsDataUrl(file);
    const url = await api.uploadMedia(dataUrl, file.name);
    const isVideo = file.type.startsWith("video/");
    const md = isVideo ? `![video:${file.name}](${url})` : `![${file.name}](${url})`;
    text.value = text.value ? `${text.value}
${md}` : md;
  } catch (err) {
    mediaError.value = String(err);
  } finally {
    uploading.value = false;
  }
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ""));
    reader.onerror = () => reject(reader.error || new Error("read failed"));
    reader.readAsDataURL(file);
  });
}

async function submit() {
  const t = text.value.trim();
  if (!t || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await api.publishPost(t, isPublic.value);
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
      maxlength="50000"
      :placeholder="t('composerPlaceholder')"
      @keydown.ctrl.enter="submit"
    ></textarea>
    <div class="row">
      <label class="public-toggle">
        <input v-model="isPublic" type="checkbox" />
        公开
      </label>
      <span class="hint">{{ t("composerHint") }}</span>
      <span v-if="mediaError" class="error">{{ mediaError }}</span>
      <span v-if="error" class="error">{{ error }}</span>
      <input ref="fileInput" type="file" accept="image/*,video/*" hidden @change="onFileSelected" />
      <button :disabled="uploading" @click="fileInput?.click()">
        {{ uploading ? t("uploading") : t("imageVideo") }}
      </button>
      <button @click="insertImageUrl">图片 URL</button>
      <button class="primary" :disabled="busy || uploading || !text.trim()" @click="submit">
        {{ busy ? t("sending") : t("publish") }}
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
.public-toggle {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--text-dim);
  font-size: 12px;
  white-space: nowrap;
}
</style>
