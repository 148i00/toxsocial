<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api, formatTime } from "../api";
import { t } from "../i18n";
import { renderMarkdown } from "../markdown";
import Avatar from "./Avatar.vue";
import type { TimelineItem } from "../types";

function md(text?: string | null): string {
  return renderMarkdown(text || "");
}

const props = defineProps<{ postId: string }>();
const emit = defineEmits<{ refresh: []; author: [pubkey: string] }>();

const post = ref<TimelineItem | null>(null);
const comments = ref<TimelineItem[]>([]);
const reactions = ref<TimelineItem[]>([]);
const commentText = ref("");
const replyTarget = ref<string | null>(null);
const commentInput = ref<HTMLTextAreaElement | null>(null);
const busy = ref(false);
const sortMode = ref<"time" | "score">("time");
const reverse = ref(false);

// --- like / dislike helpers --------------------------------------------------

function countsOf(item: TimelineItem): { like: number; dislike: number; liked: boolean; disliked: boolean } {
  const like = item.reactions.filter((r) => r.emoji === "👍").reduce((n, r) => n + r.count, 0);
  const dislike = item.reactions.filter((r) => r.emoji === "👎").reduce((n, r) => n + r.count, 0);
  return {
    like,
    dislike,
    liked: item.reactions.some((r) => r.emoji === "👍" && r.mine),
    disliked: item.reactions.some((r) => r.emoji === "👎" && r.mine),
  };
}

function scoreOf(item: TimelineItem): number {
  const c = countsOf(item);
  return c.like - c.dislike;
}

const postVotes = computed(() => (post.value ? countsOf(post.value) : null));

async function react(itemId: string, emoji: string) {
  try {
    await api.publishReaction(itemId, emoji);
    await load();
    emit("refresh");
  } catch {
    /* ignore */
  }
}

// --- comment depth & sorting -------------------------------------------------

const commentDepth = computed(() => {
  const depth = new Map<string, number>();
  for (const c of comments.value) {
    let d = 0;
    let cur: string | null = c.parentId;
    let guard = 0;
    while (cur && cur !== props.postId && guard < 20) {
      d++;
      cur = comments.value.find((x) => x.id === cur)?.parentId ?? null;
      guard++;
    }
    depth.set(c.id, d);
  }
  return depth;
});

/** Comments sorted by time or like/dislike score; both support reverse. */
const sortedComments = computed(() => {
  const list = [...comments.value];
  if (sortMode.value === "time") {
    list.sort((a, b) => a.ts - b.ts);
  } else {
    list.sort((a, b) => scoreOf(b) - scoreOf(a));
  }
  if (reverse.value) list.reverse();
  return list;
});

function parentCommentName(c: TimelineItem): string {
  if (!c.parentId || c.parentId === props.postId) return "";
  const p = comments.value.find((x) => x.id === c.parentId);
  return p?.authorName || "";
}

// --- load / actions ----------------------------------------------------------

async function load() {
  const items = await api.fetchThread(props.postId);
  post.value = items.find((i) => i.kind === "post") ?? null;
  comments.value = items.filter((i) => i.kind === "comment");
  reactions.value = items.filter((i) => i.kind === "reaction");
}

function replyTo(c: TimelineItem) {
  replyTarget.value = c.id;
  commentText.value = `@${c.authorName} `;
  commentInput.value?.focus();
}

function replyToPost() {
  replyTarget.value = null;
  commentText.value = "";
  commentInput.value?.focus();
}

async function submitComment() {
  const t = commentText.value.trim();
  if (!t || busy.value) return;
  busy.value = true;
  try {
    await api.publishComment(props.postId, t, replyTarget.value ?? undefined);
    commentText.value = "";
    replyTarget.value = null;
    await load();
    emit("refresh");
  } catch (e) {
    alert(String(e));
  } finally {
    busy.value = false;
  }
}

onMounted(load);
</script>

<template>
  <div v-if="post" class="thread">
    <article class="card post">
      <div class="head">
        <span class="clickable" @click="emit('author', post.author)">
          <Avatar :src="post.authorAvatar" :name="post.authorName" :size="28" />
        </span>
        <span class="author clickable" @click="emit('author', post.author)">{{ post.authorName }}</span>
        <span v-if="post.isOwn" class="tag">{{ t("me") }}</span>
        <span class="time">{{ formatTime(post.ts) }}</span>
        <span v-if="!post.tsVerified" class="tag warn" :title="t('timeUnverifiedTitle')">{{ t("timeUnverified") }}</span>
      </div>
      <div class="body markdown" v-html="md(post.text)"></div>
      <div class="stats">
        <button class="mini vote" :class="{ active: postVotes?.liked }" :title="t('like')" @click="react(post.id, '👍')">
          👍 {{ postVotes?.like || "" }}
        </button>
        <button class="mini vote" :class="{ active: postVotes?.disliked }" :title="t('dislike')" @click="react(post.id, '👎')">
          👎 {{ postVotes?.dislike || "" }}
        </button>
        <span>💬 {{ t("commentCount", { count: comments.length }) }}</span>
      </div>
    </article>

    <div v-if="replyTarget" class="reply-hint">
      {{ t("replyingTo") }}
      <strong>{{ comments.find((c) => c.id === replyTarget)?.authorName || t("thatComment") }}</strong>
      <button class="mini" @click="replyToPost">{{ t("cancel") }}</button>
    </div>

    <div class="composer">
      <textarea
        ref="commentInput"
        v-model="commentText"
        rows="2"
        maxlength="5000"
        :placeholder="replyTarget ? t('replyCommentPlaceholder') : t('commentPlaceholder')"
        @keydown.enter.exact.prevent="submitComment"
      ></textarea>
      <button class="primary" :disabled="busy || !commentText.trim()" @click="submitComment">
        {{ t("comment") }}
      </button>
    </div>

    <div class="sort-bar">
      <span class="sort-label">{{ t("sortBy") }}</span>
      <button class="mini" :class="{ active: sortMode === 'time' }" @click="sortMode = 'time'; reverse = false">{{ t("sortTime") }}</button>
      <button class="mini" :class="{ active: sortMode === 'score' }" @click="sortMode = 'score'; reverse = false">{{ t("sortScore") }}</button>
      <button class="mini" :class="{ active: reverse }" @click="reverse = !reverse">{{ reverse ? t("sortAsc") : t("sortDesc") }}</button>
    </div>

    <div v-if="comments.length === 0" class="empty">{{ t("noCommentsYet") }}</div>
    <article
      v-for="c in sortedComments"
      :key="c.id"
      class="comment"
      :style="{ marginLeft: ((commentDepth.get(c.id) || 0) * 22) + 'px' }"
    >
      <div class="head">
        <span class="clickable" @click="emit('author', c.author)">
          <Avatar :src="c.authorAvatar" :name="c.authorName" :size="22" />
        </span>
        <span class="author clickable" @click="emit('author', c.author)">{{ c.authorName }}</span>
        <span v-if="parentCommentName(c)" class="reply-to">{{ t("replyToName", { name: parentCommentName(c) }) }}</span>
        <span class="time">{{ formatTime(c.ts) }}</span>
        <span v-if="!c.tsVerified" class="tag warn" :title="t('timeUnverifiedTitle')">{{ t("timeUnverified") }}</span>
      </div>
      <div class="body markdown" v-html="md(c.text)"></div>
      <div class="comment-actions">
        <button class="mini vote" :class="{ active: countsOf(c).liked }" :title="t('like')" @click="react(c.id, '👍')">👍 {{ countsOf(c).like || "" }}</button>
        <button class="mini vote" :class="{ active: countsOf(c).disliked }" :title="t('dislike')" @click="react(c.id, '👎')">👎 {{ countsOf(c).dislike || "" }}</button>
        <button class="mini" @click="replyTo(c)">{{ t("reply") }}</button>
      </div>
    </article>
  </div>
</template>

<style scoped>
.card {
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px 14px;
  margin-bottom: 10px;
}
.tag.warn {
  color: #b8860b;
  border: 1px solid #b8860b55;
  border-radius: 8px;
  font-size: 11px;
  padding: 0 6px;
}
.clickable {
  cursor: pointer;
}
.clickable:hover {
  text-decoration: underline;
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
.stats {
  display: flex;
  align-items: center;
  gap: 10px;
  color: var(--text-dim);
  font-size: 12px;
  flex-wrap: wrap;
}
button.mini {
  padding: 2px 8px;
  font-size: 11px;
}
button.vote.active {
  background: var(--accent);
  color: #fff;
}
/* Reddit-style threaded comments: indent + a thin left rule, no card box. */
.comment {
  background: transparent;
  border: none;
  border-radius: 0;
  padding: 8px 0 6px 12px;
  margin-bottom: 2px;
  border-left: 1px solid var(--border);
}
.comment .body {
  margin: 4px 0;
  font-size: 13px;
}
.comment .head .author {
  font-size: 12px;
}
.reply-to {
  color: var(--text-dim);
  font-size: 12px;
}
.reply-hint {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-dim);
  background: var(--bg-3);
  border-radius: 8px;
  padding: 6px 10px;
  margin-bottom: 8px;
}
.comment-actions {
  margin-top: 4px;
  display: flex;
  gap: 6px;
}
.sort-bar {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 8px 0;
  font-size: 12px;
}
.sort-label {
  color: var(--text-dim);
}
.composer {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
  align-items: flex-start;
}
.composer textarea {
  flex: 1;
  min-height: 42px;
  resize: vertical;
}
.composer button {
  align-self: flex-end;
}
</style>
