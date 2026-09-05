<script setup lang="ts">
import { computed, ref } from "vue";
import { api, formatTime } from "../api";
import { t } from "../i18n";
import { renderMarkdown } from "../markdown";
import Avatar from "./Avatar.vue";
import type { OwnInfo, TimelineItem } from "../types";

const props = defineProps<{ item: TimelineItem; own: OwnInfo | null }>();
const emit = defineEmits<{
  open: [id: string];
  reacted: [];
  attachmentRequested: [postId: string];
  author: [pubkey: string];
  forward: [item: TimelineItem];
}>();

const EMOJIS = ["👍", "👎"];

/** Like/dislike counts from the reaction list. */
const likeCount = computed(() =>
  props.item.reactions.filter((r) => r.emoji === "👍").reduce((n, r) => n + r.count, 0),
);
const dislikeCount = computed(() =>
  props.item.reactions.filter((r) => r.emoji === "👎").reduce((n, r) => n + r.count, 0),
);

const bodyHtml = computed(() => renderMarkdown(props.item.text || ""));
const downloading = ref(false);

/** "name|size" -> display name */
function attachName(meta: string): string {
  return meta.split("|")[0] || meta;
}

/** "name|size" -> human-readable size */
function attachSize(meta: string): string {
  const size = Number(meta.split("|")[1] || 0);
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

async function requestAttachment() {
  try {
    await api.requestAttachment(props.item.id);
    emit("attachmentRequested", props.item.id);
  } catch (e) {
    alert(String(e));
  }
}

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
      <span class="clickable" @click.stop="emit('author', item.author)">
        <Avatar :src="item.authorAvatar" :name="item.authorName" :size="28" />
      </span>
      <span class="author clickable" @click.stop="emit('author', item.author)">{{ item.authorName }}</span>
      <span v-if="item.isOwn" class="tag">{{ t("me") }}</span>
      <span class="time">{{ formatTime(item.ts) }}</span>
      <span v-if="!item.tsVerified" class="tag warn" :title="t('timeUnverifiedTitle')">{{ t("timeUnverified") }}</span>
    </div>
    <div class="body markdown" v-html="bodyHtml"></div>
    <div v-if="item.attachment" class="attach" @click.stop>
      <span class="attach-name" :title="attachName(item.attachment)">📎 {{ attachName(item.attachment) }}</span>
      <span class="attach-size">{{ attachSize(item.attachment) }}</span>
      <button class="mini" :disabled="downloading" @click="requestAttachment">
        {{ downloading ? t("processing") : t("download") }}
      </button>
    </div>
    <div class="foot">
      <span class="stat">💬 {{ item.commentCount }}</span>
      <button v-if="!item.isOwn" class="mini forward-btn" :title="t('forward')" @click.stop="emit('forward', item)">↩ {{ t("forward") }}</button>
      <span class="actions" @click.stop>
        <button class="mini vote" :class="{ active: item.reactions.some((r) => r.emoji === '👍' && r.mine) }" :title="t('like')" @click="react('👍')">
          👍 {{ likeCount || "" }}
        </button>
        <button class="mini vote" :class="{ active: item.reactions.some((r) => r.emoji === '👎' && r.mine) }" :title="t('dislike')" @click="react('👎')">
          👎 {{ dislikeCount || "" }}
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
.clickable {
  cursor: pointer;
}
.clickable:hover {
  text-decoration: underline;
}
.actions button.vote.active {
  background: var(--accent);
  color: #fff;
}
.forward-btn {
  color: var(--text-dim);
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
.attach {
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--bg-3);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 6px 10px;
  margin: 6px 0;
  font-size: 13px;
}
.attach-name {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 60%;
}
.attach-size {
  color: var(--text-dim);
  font-size: 12px;
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
