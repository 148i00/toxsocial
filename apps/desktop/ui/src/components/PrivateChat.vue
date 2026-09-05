<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { api, formatTime, onEvent } from "../api";
import { t } from "../i18n";
import { renderMarkdown } from "../markdown";
import Avatar from "./Avatar.vue";
import type { PrivateMessageInfo } from "../types";

function md(text: string | null): string {
  return renderMarkdown(text || "");
}

const props = defineProps<{ peer: string; name: string }>();
const emit = defineEmits<{ close: [] }>();

const messages = ref<PrivateMessageInfo[]>([]);
const text = ref("");
const busy = ref(false);
const error = ref("");
const listRef = ref<HTMLElement | null>(null);

function scrollBottom() {
  nextTick(() => listRef.value?.scrollTo({ top: listRef.value.scrollHeight }));
}

async function load() {
  try {
    messages.value = await api.privateMessages(props.peer, 200);
    scrollBottom();
  } catch {
    /* ignore */
  }
}

async function send() {
  const t = text.value.trim();
  if (!t || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const id = await api.sendPrivateMessage(props.peer, t);
    messages.value.push({ id, text: t, ts: Date.now(), direction: 1 });
    text.value = "";
    scrollBottom();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

let unlisten: Awaited<ReturnType<typeof onEvent>> | undefined;

watch(
  () => props.peer,
  () => load(),
);

onMounted(async () => {
  await load();
  unlisten = await onEvent<{ peer: string; text: string; id: number; ts: number }>(
    "pm:message",
    (e) => {
      if (e.peer !== props.peer) return;
      if (messages.value.some((m) => m.id === e.id)) return;
      messages.value.push({ id: e.id, text: e.text, ts: e.ts, direction: 0 });
      scrollBottom();
    },
  );
});

onBeforeUnmount(() => {
  unlisten?.();
});
</script>

<template>
  <div class="pm-view">
    <div class="pm-header">
      <button @click="emit('close')">{{ t("backToFriends") }}</button>
      <Avatar :name="name" :size="26" />
      <span class="pm-name">{{ name || props.peer.slice(0, 8) }}</span>
      <span class="pm-status">{{ t("pmEncrypted") }}</span>
    </div>

    <div ref="listRef" class="pm-messages">
      <div v-if="messages.length === 0" class="empty">{{ t("noPmYet") }}</div>
      <div
        v-for="m in messages"
        :key="m.id || m.ts"
        class="chat-msg"
        :class="{ mine: m.direction === 1 }"
      >
        <div class="bubble">
          <div class="bubble-peer">
            {{ m.direction === 1 ? t("me") : name || props.peer.slice(0, 8) }}
            <span class="pm-time">{{ formatTime(m.ts) }}</span>
          </div>
          <div class="bubble-text markdown" v-html="md(m.text)"></div>
        </div>
      </div>
    </div>

    <div class="pm-composer">
      <textarea
        v-model="text"
        rows="2"
        maxlength="1300"
        :placeholder="t('pmPlaceholder')"
        @keydown.enter.exact.prevent="send"
      ></textarea>
      <button class="primary" :disabled="busy || !text.trim()" @click="send">
        {{ t("send") }}
      </button>
    </div>
    <span v-if="error" class="pm-error">{{ error }}</span>
  </div>
</template>

<style scoped>
.pm-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.pm-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--border);
}
.pm-name {
  font-weight: 700;
}
.pm-status {
  color: var(--text-dim);
  font-size: 12px;
  margin-left: auto;
}
.pm-messages {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.pm-time {
  color: var(--text-dim);
  font-size: 11px;
  margin-left: 6px;
}
.pm-composer {
  display: flex;
  gap: 8px;
  padding: 10px 12px;
  border-top: 1px solid var(--border);
  align-items: flex-end;
}
.pm-composer textarea {
  flex: 1;
  min-height: 40px;
  resize: vertical;
}
.pm-error {
  color: var(--danger);
  font-size: 12px;
  padding: 0 12px 8px;
}
</style>
