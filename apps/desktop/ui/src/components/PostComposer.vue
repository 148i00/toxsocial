<script setup lang="ts">
import { ref, watch } from "vue";
import { api } from "../api";
import { t } from "../i18n";
import type { OwnInfo } from "../types";

const props = defineProps<{
  own: OwnInfo | null;
  /** When set, posts are scoped to this community. */
  community?: string;
  communityName?: string;
  /** Prefill text (e.g. a forwarded post quote). */
  prefill?: string;
}>();
const emit = defineEmits<{ posted: [] }>();

const text = ref("");
const busy = ref(false);
const error = ref("");
const uploading = ref(false);
const isPublic = ref(false);
const mediaError = ref("");
const fileInput = ref<HTMLInputElement | null>(null);
const attachInput = ref<HTMLInputElement | null>(null);
const attachFile = ref<{ name: string; size: number; dataUrl: string } | null>(null);

function insertImageUrl() {
  const url = prompt(t("imageUrlPrompt"));
  if (!url) return;
  const md = `![${t("image")}](${url.trim()})`;
  text.value = text.value ? `${text.value}
${md}` : md;
}

async function onAttachSelected(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file) return;
  if (file.size > 20 * 1024 * 1024) {
    mediaError.value = t("fileTooLarge", { size: "20MB" });
    return;
  }
  const dataUrl = await readFileAsDataUrl(file);
  attachFile.value = { name: file.name, size: file.size, dataUrl };
  mediaError.value = "";
}

function formatAttachSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

async function onFileSelected(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file || uploading.value) return;
  if (file.size > 10 * 1024 * 1024) {
    mediaError.value = t("fileTooLarge", { size: "10MB" });
    return;
  }
  uploading.value = true;
  mediaError.value = "";
  try {
    const dataUrl = await readFileAsDataUrl(file);
    const url = await api.uploadMedia(dataUrl, file.name);
    const isVideo = file.type.startsWith("video/");
    const md = isVideo ? `![${t("video")}:${file.name}](${url})` : `![${file.name}](${url})`;
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
    reader.onerror = () => reject(reader.error || new Error(t("readFailed")));
    reader.readAsDataURL(file);
  });
}

// Forward prefill: when the parent hands us a quoted post, drop it in.
watch(
  () => props.prefill,
  (v) => {
    if (v) text.value = v;
  },
  { immediate: true },
);

async function submit() {
  const t = text.value.trim();
  if ((!t && !attachFile.value) || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    // Community scope implies public visibility.
    await api.publishPost(t, isPublic.value || !!props.community, attachFile.value?.dataUrl, attachFile.value?.name, props.community);
    text.value = "";
    attachFile.value = null;
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
      <label v-if="!community" class="public-toggle">
        <input v-model="isPublic" type="checkbox" />
        {{ t("publicLabel") }}
      </label>
      <span v-else class="public-toggle">📢 {{ t("postingToCommunity", { name: communityName }) }}</span>
      <span class="hint">{{ t("composerHint") }}</span>
      <span v-if="mediaError" class="error">{{ mediaError }}</span>
      <span v-if="error" class="error">{{ error }}</span>
      <input ref="fileInput" type="file" accept="image/*,video/*" hidden @change="onFileSelected" />
      <button :disabled="uploading" @click="fileInput?.click()">
        {{ uploading ? t("uploading") : t("imageVideo") }}
      </button>
      <button @click="insertImageUrl">{{ t("imageUrl") }}</button>
      <input ref="attachInput" type="file" hidden @change="onAttachSelected" />
      <button class="attach-btn" @click="attachInput?.click()">📎 {{ t("attachment") }}</button>
      <span v-if="attachFile" class="attach-chip" :title="attachFile.name">
        📎 {{ attachFile.name }} ({{ formatAttachSize(attachFile.size) }})
        <button class="mini" @click="attachFile = null">✕</button>
      </span>
      <button class="primary" :disabled="busy || uploading || (!text.trim() && !attachFile)" @click="submit">
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
  /* Overflow must wrap to a new line — shrinking to min-content turns CJK
     text into one-glyph-per-line vertical columns. */
  flex-wrap: wrap;
}
.row button {
  flex-shrink: 0;
  white-space: nowrap;
}
.hint {
  color: var(--text-dim);
  font-size: 12px;
  /* Grow to fill the line, but claim a readable minimum: below it the hint
     wraps onto its own full-width line instead of crushing. */
  flex: 1 1 240px;
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
  flex-shrink: 0;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
}
.attach-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: var(--bg-3);
  border: 1px solid var(--border);
  border-radius: 999px;
  padding: 2px 8px;
  font-size: 12px;
  max-width: 220px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
