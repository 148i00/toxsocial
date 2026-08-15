<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, onEvent } from "./api";
import { t } from "./i18n";
import type { FriendInfo, OwnInfo, TimelineItem } from "./types";
import PostComposer from "./components/PostComposer.vue";
import PostCard from "./components/PostCard.vue";
import ThreadView from "./components/ThreadView.vue";
import FriendsPanel from "./components/FriendsPanel.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import ChannelsPanel from "./components/ChannelsPanel.vue";
import Avatar from "./components/Avatar.vue";

const view = ref<"timeline" | "friends" | "settings" | "channels">("timeline");
const own = ref<OwnInfo | null>(null);
const timeline = ref<TimelineItem[]>([]);
const threadPostId = ref<string | null>(null);
const friends = ref<FriendInfo[]>([]);
const loading = ref(true);
const searchQuery = ref("");
const searchResults = ref<TimelineItem[]>([]);
const searching = ref(false);
const notifications = ref<{ id: number; text: string; time: number }[]>([]);
const unread = ref(0);
const showNotifications = ref(false);
let notificationId = 0;

function notify(text: string) {
  notifications.value.unshift({ id: ++notificationId, text, time: Date.now() });
  unread.value++;
}

function markAllRead() {
  unread.value = 0;
}

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
  onEvent("feed:post", () => {
    refreshTimeline();
    notify("收到新帖子");
  });
  onEvent("feed:comment", () => {
    refreshTimeline();
    if (threadPostId.value) refreshThread();
    notify("收到新评论");
  });
  onEvent("feed:reaction", () => {
    refreshTimeline();
    if (threadPostId.value) refreshThread();
    notify("收到新反应");
  });
  onEvent("friend:connection", () => {
    refreshFriends();
    refreshOwn();
    notify("好友连接状态变化");
  });
  onEvent("friend:request", () => {
    refreshFriends();
    refreshOwn();
    notify("收到好友请求");
  });
  onEvent("friend:name", () => {
    refreshFriends();
    refreshTimeline();
    notify("好友更新了昵称");
  });
  onEvent("channel:message", () => notify("收到频道消息"));
  onEvent("channel:connected", () => notify("已连接频道"));
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

async function runSearch() {
  const q = searchQuery.value.trim();
  if (!q) {
    searchResults.value = [];
    return;
  }
  searching.value = true;
  try {
    searchResults.value = await api.searchPosts(q, 50);
  } catch {
    searchResults.value = [];
  } finally {
    searching.value = false;
  }
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
        <button :class="{ active: view === 'channels' }" @click="view = 'channels'">{{ t("channels") }}</button>
        <button @click="showNotifications = !showNotifications">
          通知 <span v-if="unread" class="count">{{ unread }}</span>
        </button>
        <button :class="{ active: view === 'settings' }" @click="view = 'settings'">{{ t("settings") }}</button>
      </nav>
      <div v-if="showNotifications" class="notif-panel">
        <div class="notif-head">
          <span>通知</span>
          <button class="mini" @click="markAllRead">全部已读</button>
        </div>
        <div v-if="notifications.length === 0" class="empty">暂无通知</div>
        <div v-for="n in notifications" :key="n.id" class="notif-item">
          <span>{{ n.text }}</span>
          <span class="time">{{ new Date(n.time).toLocaleTimeString() }}</span>
        </div>
      </div>
      <div class="sidebar-foot" v-if="own">
        <Avatar :src="own.avatar" :name="own.name" :size="28" />
        <div class="foot-info">
          <span class="foot-name">{{ own.name || "未设置昵称" }}</span>
          <span class="tag">{{ t("connectedDht") }}</span>
        </div>
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
          <div class="search-box">
            <input
              v-model="searchQuery"
              :placeholder="t('searchPlaceholder')"
              @input="runSearch"
            />
          </div>
          <template v-if="searchQuery.trim()">
            <div v-if="searching" class="empty">{{ t("searching") }}</div>
            <div v-else-if="searchResults.length === 0" class="empty">{{ t("noSearchResults") }}</div>
            <PostCard
              v-for="p in searchResults"
              :key="p.id"
              :item="p"
              :own="own"
              @open="openThreadWithData"
            />
          </template>
          <template v-else>
            <PostComposer :own="own" @posted="refreshTimeline" />
            <div v-if="loading" class="empty">{{ t("loading") }}</div>
            <div v-else-if="timeline.length === 0" class="empty">
              {{ t("emptyTimeline") }}
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
      </template>

      <FriendsPanel v-else-if="view === 'friends'" :friends="friends" @changed="refreshAll" />
      <ChannelsPanel v-else-if="view === 'channels'" />
      <SettingsPanel v-else :own="own" @saved="refreshAll" />
    </main>

    <!-- Right: me panel -->
    <aside class="me-panel" v-if="own">
      <div class="me-card">
        <Avatar :src="own.avatar" :name="own.name" :size="64" />
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
.foot-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.foot-name {
  color: var(--text);
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.content {
  overflow-y: auto;
  border-right: 1px solid var(--border);
  padding: 16px 20px 40px;
}

.thread-header {
  margin-bottom: 10px;
}
.search-box {
  margin-bottom: 12px;
}
.notif-panel {
  background: var(--bg-3);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 8px;
  max-height: 260px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.notif-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-weight: 600;
  font-size: 13px;
  margin-bottom: 4px;
}
.notif-item {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  padding: 4px 6px;
  border-radius: 6px;
  background: var(--bg-2);
}
.notif-item .time {
  color: var(--text-dim);
  white-space: nowrap;
}
button.mini {
  padding: 2px 8px;
  font-size: 11px;
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
