<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, onEvent } from "./api";
import type { FriendInfo, OwnInfo, TimelineItem } from "./types";
import PostComposer from "./components/PostComposer.vue";
import PostCard from "./components/PostCard.vue";
import ThreadView from "./components/ThreadView.vue";
import FriendsPanel from "./components/FriendsPanel.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import ChannelsPanel from "./components/ChannelsPanel.vue";

const view = ref<"timeline" | "friends" | "settings" | "channels">("timeline");
const own = ref<OwnInfo | null>(null);
const timeline = ref<TimelineItem[]>([]);
const threadPostId = ref<string | null>(null);
const friends = ref<FriendInfo[]>([]);
const loading = ref(true);

async function refreshOwn() {
  own.value = await api.getOwnInfo();
}

async function refreshTimeline() {
  timeline.value = await api.fetchTimeline(50);
}

async function refreshFriends() {
  friends.value = await api.getFriends();
}

async function refreshAll() {
  await Promise.all([refreshOwn(), refreshTimeline(), refreshFriends()]);
}

function openThread(id: string) {
  threadPostId.value = id;
  view.value = "timeline"; // thread is rendered inside the timeline view
}

function backToTimeline() {
  threadPostId.value = null;
}

onMounted(async () => {
  await refreshAll();
  loading.value = false;

  // Live updates from the backend.
  onEvent("feed:post", () => refreshTimeline());
  onEvent("feed:comment", () => {
    refreshTimeline();
    if (threadPostId.value) refreshThread();
  });
  onEvent("feed:reaction", () => {
    refreshTimeline();
    if (threadPostId.value) refreshThread();
  });
  onEvent("friend:connection", () => {
    refreshFriends();
    refreshOwn();
  });
  onEvent("friend:request", () => {
    refreshFriends();
    refreshOwn();
  });
  onEvent("friend:name", () => {
    refreshFriends();
    refreshTimeline();
  });
});

const threadItems = ref<TimelineItem[]>([]);
async function refreshThread() {
  if (!threadPostId.value) return;
  threadItems.value = await api.fetchThread(threadPostId.value);
}

async function openThreadWithData(id: string) {
  threadPostId.value = id;
  threadItems.value = await api.fetchThread(id);
}
</script>

<template>
  <div class="layout">
    <!-- Left: navigation -->
    <aside class="sidebar">
      <div class="logo">🦊 ToxSocial</div>
      <nav>
        <button :class="{ active: view === 'timeline' && !threadPostId }" @click="backToTimeline(); view = 'timeline'">
          首页
        </button>
        <button :class="{ active: view === 'friends' }" @click="view = 'friends'">
          关注 <span v-if="own" class="count">{{ own.friendCount }}</span>
        </button>
        <button :class="{ active: view === 'channels' }" @click="view = 'channels'">频道</button>
        <button :class="{ active: view === 'settings' }" @click="view = 'settings'">设置</button>
      </nav>
      <div class="sidebar-foot" v-if="own">
        <div class="dot" :class="{ online: own.friendCount > 0 }"></div>
        <span>{{ own.name || "未设置昵称" }}</span>
        <span class="tag">已连接 DHT</span>
      </div>
    </aside>

    <!-- Center: content -->
    <main class="content">
      <!-- Timeline / thread -->
      <template v-if="view === 'timeline'">
        <div v-if="threadPostId" class="thread-header">
          <button @click="backToTimeline()">← 返回时间线</button>
        </div>
        <ThreadView v-if="threadPostId" :post-id="threadPostId" @refresh="refreshThread" />
        <template v-else>
          <PostComposer :own="own" @posted="refreshTimeline" />
          <div v-if="loading" class="empty">加载中…</div>
          <div v-else-if="timeline.length === 0" class="empty">
            时间线还是空的 — 添加好友并等待对方发帖吧。
          </div>
          <PostCard
            v-for="p in timeline"
            :key="p.id"
            :item="p"
            :own="own"
            @open="openThreadWithData"
          />
        </template>
      </template>

      <FriendsPanel v-else-if="view === 'friends'" :friends="friends" @changed="refreshAll" />
      <ChannelsPanel v-else-if="view === 'channels'" />
      <SettingsPanel v-else :own="own" @saved="refreshAll" />
    </main>

    <!-- Right: me panel -->
    <aside class="me-panel" v-if="own">
      <div class="me-card">
        <div class="me-name">{{ own.name || "未设置昵称" }}</div>
        <div class="mono">{{ own.pubkey.slice(0, 20) }}…</div>
        <div class="me-stats">
          <span>好友 {{ own.friendCount }}</span>
        </div>
      </div>
    </aside>
  </div>
</template>

<style scoped>
.layout {
  display: grid;
  grid-template-columns: 210px 1fr 260px;
  height: 100%;
}

.sidebar {
  background: var(--bg-2);
  border-right: 1px solid var(--border);
  padding: 16px 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.logo {
  font-size: 17px;
  font-weight: 700;
  padding: 4px 8px;
}

nav {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

nav button {
  text-align: left;
  font-size: 14px;
  padding: 10px 12px;
}
nav button.active {
  background: var(--bg-3);
  color: var(--accent);
  font-weight: 600;
}
.count {
  background: var(--bg-3);
  border-radius: 999px;
  padding: 1px 8px;
  font-size: 12px;
  margin-left: 6px;
}

.sidebar-foot {
  margin-top: auto;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 8px;
  font-size: 13px;
  color: var(--text-dim);
}

.content {
  overflow-y: auto;
  border-right: 1px solid var(--border);
  padding: 16px 20px 40px;
}

.thread-header {
  margin-bottom: 10px;
}

.me-panel {
  background: var(--bg-2);
  padding: 16px;
}
.me-card {
  background: var(--bg-3);
  border-radius: var(--radius);
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.me-name {
  font-size: 15px;
  font-weight: 600;
}
.me-stats {
  color: var(--text-dim);
  font-size: 12px;
  margin-top: 4px;
}
</style>
