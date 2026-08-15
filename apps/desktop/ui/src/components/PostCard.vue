<script setup lang="ts">
import { api, formatTime } from "../api";
import type { OwnInfo, TimelineItem } from "../types";

const props = defineProps<{ item: TimelineItem; own: OwnInfo | null }>();
const emit = defineEmits<{ open: [id: string] }>();

const EMOJIS = ["👍", "❤️", "😂", "🔥", "🎉"];

async function react(emoji: string) {
  try {
    await api.publishReaction(props.item.id, emoji);
  } catch {
    /* ignore */
  }
}
</script>

<template>
  <article class="card" @click="emit('open', item.id)">
    <div class="head">
      <span class="author">{{ item.authorName }}</span>
      <span v-if="item.isOwn" class="tag">我</span>
      <span class="time">{{ formatTime(item.ts) }}</span>
    </div>
    <div class="body">{{ item.text }}</div>
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
          :title="`反应：${e}`"
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
