<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { api, formatTime } from "../api";
import { renderMarkdown } from "../markdown";
import Avatar from "./Avatar.vue";
import type { TimelineItem } from "../types";

function md(text?: string | null): string {
  return renderMarkdown(text || "");
}

const props = defineProps<{ postId: string }>();
const emit = defineEmits<{ refresh: [] }>();

const post = ref<TimelineItem | null>(null);
const comments = ref<TimelineItem[]>([]);
const reactions = ref<TimelineItem[]>([]);
const commentText = ref("");
const replyTarget = ref<string | null>(null);
const commentInput = ref<HTMLTextAreaElement | null>(null);
const busy = ref(false);

const reactionSummary = computed(() => {
  const counts = new Map<string, number>();
  for (const r of reactions.value) {
    const e = r.emoji || "?";
    counts.set(e, (counts.get(e) ?? 0) + 1);
  }
  return Array.from(counts.entries())
    .map(([emoji, count]) => (count > 1 ? `${emoji} ${count}` : emoji))
    .join("  ");
});

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

function parentCommentName(c: TimelineItem): string {
  if (!c.parentId || c.parentId === props.postId) return "";
  const p = comments.value.find((x) => x.id === c.parentId);
  return p?.authorName || "";
}

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
        <Avatar :src="post.authorAvatar" :name="post.authorName" :size="28" />
        <span class="author">{{ post.authorName }}</span>
        <span v-if="post.isOwn" class="tag">我</span>
        <span class="time">{{ formatTime(post.ts) }}</span>
      </div>
      <div class="body markdown" v-html="md(post.text)"></div>
      <div class="stats">
        <span>💬 {{ comments.length }} 评论</span>
        <span>⚡ {{ reactions.length }} 反应：{{ reactionSummary || "暂无" }}</span>
      </div>
    </article>

    <div v-if="replyTarget" class="reply-hint">
      正在回复
      <strong>{{ comments.find((c) => c.id === replyTarget)?.authorName || "该评论" }}</strong>
      <button class="mini" @click="replyToPost">取消</button>
    </div>

    <div class="composer">
      <textarea
        ref="commentInput"
        v-model="commentText"
        rows="2"
        maxlength="5000"
        :placeholder="replyTarget ? '回复该评论…（Enter 发送，Shift+Enter 换行）' : '写下你的评论…（Enter 发送，Shift+Enter 换行）'"
        @keydown.enter.exact.prevent="submitComment"
      ></textarea>
      <button class="primary" :disabled="busy || !commentText.trim()" @click="submitComment">
        评论
      </button>
    </div>

    <div v-if="comments.length === 0" class="empty">还没有评论，来抢沙发 🛋️</div>
    <article
      v-for="c in comments"
      :key="c.id"
      class="card comment"
      :style="{ marginLeft: ((commentDepth.get(c.id) || 0) * 28) + 'px' }"
    >
      <div class="head">
        <Avatar :src="c.authorAvatar" :name="c.authorName" :size="24" />
        <span class="author">{{ c.authorName }}</span>
        <span v-if="parentCommentName(c)" class="reply-to">回复 @{{ parentCommentName(c) }}</span>
        <span class="time">{{ formatTime(c.ts) }}</span>
      </div>
      <div class="body markdown" v-html="md(c.text)"></div>
      <div class="comment-actions">
        <button class="mini" @click="replyTo(c)">回复</button>
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
  color: var(--text-dim);
  font-size: 12px;
  display: flex;
  gap: 16px;
}
.comment {
  border-left: 3px solid var(--accent-2);
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
  margin-top: 6px;
}
button.mini {
  padding: 2px 8px;
  font-size: 11px;
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
