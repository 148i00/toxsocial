<script setup lang="ts">
import { computed } from "vue";
import { api, formatTime } from "../api";
import { t } from "../i18n";
import { renderMarkdown } from "../markdown";
import Avatar from "./Avatar.vue";
import type { OwnInfo, TimelineItem } from "../types";

const props = defineProps<{ item: TimelineItem; own: OwnInfo | null }>();
const emit = defineEmits<{ open: [id: string]; reacted: [] }>();

const EMOJIS = ["👍", "❤️", "😂", "🔥", "🎉"];

const bodyHtml = computed(() => renderMarkdown(props.item.text || ""));

async function react(emoji: string) {
  try {
    await api.publishReaction(props.item.id, emoji);
    emit("reacted");
  } catch {
    /* ignore */
  }
}
</script>

<template>
  <article class="card" @click="emit('open', item.id)">
    <div class="head">
      <Avatar :src="item.authorAvatar" :name="item.authorName" :size="28" />
      <span class="author">{{ item.authorName }}</span>
      <span v-if="item.isOwn" class="tag">{{ t("me") }}</span>
      <span class="time">{{ formatTime(item.ts) }}</span>
      <span v-if="!item.tsVerified" class="tag warn" :title="t('timeUnverifiedTitle')">{{ t("timeUnverified") }}</span>
    </div>
    <div class="body markdown" v-html="bodyHtml"></div>
    <div class="foot">
      <span class="stat">💬 {{ item.commentCount }}</span>
      <span class="stat">⚡
        <template v-if="item.reactions.length">{{ item.reactions.map((r) => r.count > 1 ? `${r.emoji} ${r.count}` : r.emoji).join(" ") }}</template>
        <template v-else>{{ item.reactionCount }}</template>
      </span>
      <span class="actions" @click.stop>
        <button
          v-for="e in EMOJIS"
          :key="e"
          class="mini"
          :title="t('reactWith', { emoji: e })"
          @click="react(e)"
        >
          {{ e }}
        </button>
      </span>
    </div>
  </article>
</template>

<style scoped>
.card {
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px 14px;
  margin-bottom: 10px;
  cursor: pointer;
  transition: border-color 0.15s;
}
.card:hover {
  border-color: var(--accent);
}
.head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.author {
  font-weight: 600;
  font-size: 13px;
}
.time {
  color: var(--text-dim);
  font-size: 12px;
  margin-left: auto;
}
.tag.warn {
  color: #b8860b;
  border: 1px solid #b8860b55;
  border-radius: 8px;
  font-size: 11px;
  padding: 0 6px;
}
.body {
  margin: 8px 0;
  font-size: 14px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
}
.foot {
  display: flex;
  align-items: center;
  gap: 14px;
  color: var(--text-dim);
  font-size: 12px;
}
.actions {
  margin-left: auto;
  display: flex;
  gap: 4px;
}
button.mini {
  padding: 3px 7px;
  font-size: 13px;
  background: transparent;
}
button.mini:hover {
  background: var(--bg-3);
}
</style>
