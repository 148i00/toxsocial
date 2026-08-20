<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { api, onEvent } from "./api";
import { pushChannelMessage } from "./channelStore";
import { t } from "./i18n";
import type { DirectoryEntryInfo, FileTransferInfo, FriendInfo, NetworkStatus, OwnInfo, TimelineItem } from "./types";
import PostComposer from "./components/PostComposer.vue";
import PostCard from "./components/PostCard.vue";
import ThreadView from "./components/ThreadView.vue";
import FriendsPanel from "./components/FriendsPanel.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import ChannelsPanel from "./components/ChannelsPanel.vue";
import Avatar from "./components/Avatar.vue";

const view = ref<"timeline" | "friends" | "settings" | "channels" | "public" | "profile">("timeline");
const own = ref<OwnInfo | null>(null);
const networkStatus = ref<NetworkStatus | null>(null);
const timeline = ref<TimelineItem[]>([]);
const threadPostId = ref<string | null>(null);
const friends = ref<FriendInfo[]>([]);
const loading = ref(true);
const searchQuery = ref("");
const searchResults = ref<TimelineItem[]>([]);
const publicTimeline = ref<TimelineItem[]>([]);
const friendFilter = ref<string | null>(null);
const friendPosts = ref<TimelineItem[]>([]);
const profileUser = ref<{ pubkey: string; name: string; avatar: string; bio: string } | null>(null);
const searching = ref(false);
const notifications = ref<{ id: number; text: string; time: number }[]>([]);
const unread = ref(0);
const showNotifications = ref(false);
const fileRequests = ref<{ id: number; friendNumber: number; fileNumber: number; friendName: string; filename: string; fileSize: number }[]>([]);
const currentFileRequest = computed(() => fileRequests.value[0] || null);
let fileRequestId = 0;
const showAddFriend = ref(false);
const addToxid = ref("");
const addBusy = ref(false);
const addError = ref("");
const userSearchResults = ref<DirectoryEntryInfo[]>([]);
const searchingUsers = ref(false);
const followBusy = ref(false);
const transfers = ref<FileTransferInfo[]>([]);
let notificationId = 0;
let statusTimer: ReturnType<typeof setInterval> | undefined;
let transferTimer: ReturnType<typeof setInterval> | undefined;

function notify(text: string) {
  notifications.value.unshift({ id: ++notificationId, text, time: Date.now() });
  unread.value++;
  // No left-side notification button anymore; show the panel automatically
  // so new notifications are visible.
  showNotifications.value = true;
}

/** Attachment download request sent to the author. */
function onAttachmentRequested() {
  notify(t("attachmentRequested"));
}

async function refreshTransfers() {
  try {
    transfers.value = await api.fileTransfers();
  } catch {
    transfers.value = [];
  }
}

function transferPct(tr: FileTransferInfo): number {
  if (!tr.total) return 0;
  return Math.min(100, Math.round((tr.sent / tr.total) * 100));
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

async function acceptFileRequest() {
  const req = currentFileRequest.value;
  if (!req) return;
  try {
    await api.acceptFile(req.friendNumber, req.fileNumber);
    fileRequests.value = fileRequests.value.filter((r) => r.id !== req.id);
    notify(t("fileAccepted", { filename: req.filename }));
  } catch (e) {
    alert(String(e));
  }
}

async function rejectFileRequest() {
  const req = currentFileRequest.value;
  if (!req) return;
  try {
    await api.rejectFile(req.friendNumber, req.fileNumber);
    fileRequests.value = fileRequests.value.filter((r) => r.id !== req.id);
    notify(t("fileRejected", { filename: req.filename }));
  } catch (e) {
    alert(String(e));
  }
}

function markAllRead() {
  unread.value = 0;
}

async function submitAddFriend() {
  const q = addToxid.value.trim();
  if (!q || addBusy.value) return;
  addBusy.value = true;
  addError.value = "";
  userSearchResults.value = [];
  try {
    // Search only (local store + Relay directory). Results open the user's
    // profile page, where a follow button is available.
    searchingUsers.value = true;
    const local = await api.searchDirectory(q, 20);
    let relay: DirectoryEntryInfo[] = [];
    try {
      relay = await api.searchRelayDirectory(q);
    } catch {
      // ignore relay errors
    }
    const seen = new Set<string>();
    userSearchResults.value = [...local, ...relay].filter((d) => {
      if (!d.pubkey || seen.has(d.pubkey)) return false;
      seen.add(d.pubkey);
      return true;
    });
    // A full ToxID (76 hex) can still be inspected: view its profile and
    // follow from there.
    if (/^[0-9a-fA-F]{76}$/.test(q) && !seen.has(q.slice(0, 64))) {
      userSearchResults.value.unshift({
        name: t("userByToxid"),
        pubkey: q.slice(0, 64).toLowerCase(),
        toxid: q.toLowerCase(),
        avatar: "",
        relay: "",
        source: "input",
      });
    }
    addError.value = userSearchResults.value.length === 0 ? t("noUserFound") : "";
  } catch (e) {
    addError.value = String(e);
  } finally {
    addBusy.value = false;
    searchingUsers.value = false;
  }
}

/** Open a searched user's profile page (posts + follow button). */
async function viewSearchedUser(d: DirectoryEntryInfo) {
  const pubkey = (d.pubkey || "").toLowerCase();
  if (!pubkey) return;
  const f = friends.value.find((x) => x.pubkey === pubkey || x.toxid.startsWith(pubkey));
  profileUser.value = {
    pubkey,
    name: f?.name || d.name || t("unnamed"),
    avatar: f?.avatar || d.avatar || "",
    bio: f?.bio || "",
  };
  friendFilter.value = pubkey;
  friendPosts.value = await api.fetchPostsByAuthor(pubkey, 50);
  showAddFriend.value = false;
  view.value = "profile";
}

/** Follow (add as friend) the user whose profile is open. */
async function followProfileUser() {
  const u = profileUser.value;
  if (!u || followBusy.value) return;
  followBusy.value = true;
  try {
    await api.addFriend(u.pubkey, t("followMessage"));
    notify(t("followingStarted", { name: u.name }));
    await refreshAll();
  } catch (e) {
    const msg = String(e);
    if (msg.includes("已发送") || msg.includes("already")) {
      notify(t("followingStarted", { name: u.name }));
    } else {
      alert(msg);
    }
  } finally {
    followBusy.value = false;
  }
}

function isFollowingUser(pubkey: string): boolean {
  return friends.value.some(
    (f) => f.pubkey === pubkey || f.toxid.startsWith(pubkey) || pubkey.startsWith(f.pubkey),
  );
}

async function refreshOwn() {
  own.value = await api.getOwnInfo();
}

async function refreshNetworkStatus() {
  try {
    networkStatus.value = await api.getNetworkStatus();
  } catch {
    networkStatus.value = null;
  }
}

async function refreshTimeline() {
  timeline.value = await api.fetchTimeline(50);
}

async function refreshFriends() {
  friends.value = await api.getFriends();
}

async function refreshPublicTimeline() {
  publicTimeline.value = await api.fetchPublicTimeline(50);
}

async function requestPublic() {
  await Promise.all([
    api.requestPublicPosts(0, 2),
    api.fetchRelayPublicPosts(0),
  ]);
  await refreshPublicTimeline();
}

async function openPublic() {
  view.value = "public";
  await requestPublic();
}

async function viewFriend(pubkey: string) {
  const f = friends.value.find((x) => x.pubkey === pubkey);
  profileUser.value = {
    pubkey,
    name: f?.name || t("unnamed"),
    avatar: f?.avatar || "",
    bio: f?.bio || "",
  };
  friendFilter.value = pubkey;
  friendPosts.value = await api.fetchPostsByAuthor(pubkey, 50);
  view.value = "profile";
}

function backFromFriend() {
  friendFilter.value = null;
  friendPosts.value = [];
  profileUser.value = null;
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
  await refreshNetworkStatus();
  statusTimer = setInterval(() => {
    refreshNetworkStatus();
    refreshFriends();
  }, 10_000);
  transferTimer = setInterval(refreshTransfers, 3_000);
  refreshTransfers();
  loading.value = false;

  // Check for a new version once at startup (best-effort; GitHub may be
  // unreachable behind a firewall).
  api.checkUpdate().then((u) => {
    if (u.hasUpdate) {
      notify(t("updateAvailable", { version: u.latest }));
    }
  }).catch(() => {
    /* ignore network failures */
  });

  // Live updates from the backend.
  onEvent("feed:post", () => {
    refreshTimeline();
    notify(t("newPost"));
  });
  onEvent("feed:comment", () => {
    refreshTimeline();
    if (threadPostId.value) refreshThread();
    notify(t("newComment"));
  });
  onEvent("feed:reaction", () => {
    refreshTimeline();
    if (threadPostId.value) refreshThread();
    notify(t("newReaction"));
  });
  onEvent("friend:connection", () => {
    refreshFriends();
    refreshOwn();
    refreshNetworkStatus();
    notify(t("friendConnectionChanged"));
  });
  onEvent("friend:request", () => {
    refreshFriends();
    refreshOwn();
    notify(t("friendRequestReceived"));
  });
  onEvent("friend:name", () => {
    refreshFriends();
    refreshTimeline();
    notify(t("friendNameUpdated"));
  });
  onEvent("friend:bio", () => {
    refreshFriends();
  });
  onEvent("channel:message", (e: { conferenceNumber: number; channelId?: string; peerNumber: number; peerName?: string; text: string; id?: number; ts?: number }) => {
    pushChannelMessage({
      id: e.id,
      conferenceNumber: e.conferenceNumber,
      channelId: e.channelId,
      channelName: t("channelNameWithNumber", { number: e.conferenceNumber }),
      peer: e.peerName || `#${e.peerNumber}`,
      text: e.text,
      ts: e.ts,
    });
    notify(t("channelMessageReceived"));
  });
  onEvent("channel:connected", () => notify(t("channelConnected")));
  onEvent("channel:pending_flushed", (e: { count: number }) =>
    notify(t("channelPendingFlushed", { count: e.count })),
  );
  onEvent("relay:publish_failed", (e: { error: string }) =>
    notify(t("relayPublishFailed", { error: e.error })),
  );
  onEvent("post:ts_verified", () => {
    // A directly-received post was found on the Relay; its timestamp is now
    // verified, so refresh to drop the "unverified" warning.
    refreshTimeline();
    refreshPublicTimeline();
  });
  onEvent("file:request", (p: { friendNumber: number; fileNumber: number; friendName: string; filename: string; fileSize: number }) => {
    fileRequests.value.push({ id: ++fileRequestId, ...p });
    notify(t("fileRequestReceived", { filename: p.filename }));
  });
  onEvent("file:received", (p: { filename: string; path: string }) =>
    notify(t("fileReceived", { filename: p.filename, path: p.path })),
  );
});

const threadItems = ref<TimelineItem[]>([]);
async function refreshThread() {
  if (!threadPostId.value) return;
  threadItems.value = await api.fetchThread(threadPostId.value);
}

async function refreshThreadAndTimeline() {
  await Promise.all([refreshThread(), refreshTimeline()]);
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
onBeforeUnmount(() => {
  if (statusTimer) clearInterval(statusTimer);
  if (transferTimer) clearInterval(transferTimer);
});
</script>

<template>
  <div class="layout">
    <!-- Left: navigation -->
    <aside class="sidebar">
      <div class="logo">🦊 ToxSocial</div>
      <nav>
        <button :class="{ active: view === 'timeline' && !threadPostId }" @click="backToTimeline(); view = 'timeline'">
          {{ t("home") }}
        </button>
        <button @click="showAddFriend = true">{{ t("searchUsers") }}</button>
        <button :class="{ active: view === 'friends' }" @click="view = 'friends'">
          {{ t("friends") }} <span v-if="own" class="count">{{ own.friendCount }}</span>
        </button>
        <button :class="{ active: view === 'channels' }" @click="view = 'channels'">{{ t("channels") }}</button>
        <button :class="{ active: view === 'public' }" @click="openPublic">{{ t("public") }}</button>
        <button :class="{ active: view === 'settings' }" @click="view = 'settings'">{{ t("settings") }}</button>
      </nav>
      <div v-if="showNotifications" class="notif-panel">
        <div class="notif-head">
          <span>{{ t("notifications") }} <span v-if="unread" class="count">{{ unread }}</span></span>
          <button class="mini" @click="markAllRead">{{ t("markAllRead") }}</button>
          <button class="mini" @click="showNotifications = false">✕</button>
        </div>
        <div v-if="notifications.length === 0" class="empty">{{ t("noNotifications") }}</div>
        <div v-for="n in notifications" :key="n.id" class="notif-item">
          <span>{{ n.text }}</span>
          <span class="time">{{ new Date(n.time).toLocaleTimeString() }}</span>
        </div>
      </div>
      <div v-if="networkStatus && !networkStatus.relayOk" class="relay-warning">
        ⚠️ {{ t("relayUnavailable") }}
      </div>
      <div class="sidebar-foot" v-if="own">
        <span class="dot" :class="{ online: networkStatus?.connected }"></span>
        <Avatar :src="own.avatar" :name="own.name" :size="28" />
        <div class="foot-info">
          <span class="foot-name">{{ own.name || t("noNickname") }}</span>
          <span class="tag">{{ networkStatus ? (networkStatus.connected ? (networkStatus.connection === "udp" ? t("udpConnected") : t("tcpConnected")) : t("disconnected")) : t("connectedDht") }}</span>
        </div>
      </div>
    </aside>

    <!-- Center: content -->
    <main class="content">
      <!-- Timeline / thread -->
      <template v-if="view === 'timeline'">
        <div v-if="threadPostId" class="thread-header">
          <button @click="backToTimeline()">{{ t("backToTimeline") }}</button>
        </div>
        <ThreadView v-if="threadPostId" :post-id="threadPostId" @refresh="refreshThreadAndTimeline" @attachmentRequested="onAttachmentRequested" />
        <template v-else>
          <div v-if="friendFilter" class="thread-header">
            <button @click="backFromFriend()">{{ t("backToTimeline") }}</button>
            <span>{{ t("viewingFriendPosts") }}</span>
          </div>
          <template v-if="friendFilter">
            <PostCard
              v-for="p in friendPosts"
              :key="p.id"
              :item="p"
              :own="own"
              @open="openThreadWithData"
              @reacted="refreshTimeline" @attachmentRequested="onAttachmentRequested"
            />
          </template>
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
                @reacted="refreshTimeline" @attachmentRequested="onAttachmentRequested"
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
                @reacted="refreshTimeline" @attachmentRequested="onAttachmentRequested"
              />
            </template>
          </template>
        </template>
      </template>

      <div v-else-if="view === 'profile'" class="profile-page">
        <div class="thread-header">
          <button @click="backFromFriend(); view = 'friends'">{{ t("backToFriends") }}</button>
        </div>
        <div v-if="profileUser" class="profile-card">
          <Avatar :src="profileUser.avatar" :name="profileUser.name" :size="64" />
          <div>
            <div class="profile-name">{{ profileUser.name }}</div>
            <div class="mono">{{ profileUser.pubkey }}</div>
            <div v-if="profileUser.bio" class="profile-bio">{{ profileUser.bio }}</div>
          </div>
          <button
            v-if="!isFollowingUser(profileUser.pubkey)"
            class="primary follow-btn"
            :disabled="followBusy"
            @click="followProfileUser"
          >
            {{ followBusy ? t("processing") : t("follow") }}
          </button>
          <span v-else class="followed-tag">{{ t("following") }}</span>
        </div>
        <div v-if="friendPosts.length === 0" class="empty">{{ t("noPostsYet") }}</div>
        <PostCard
          v-for="p in friendPosts"
          :key="p.id"
          :item="p"
          :own="own"
          @open="openThreadWithData"
          @reacted="refreshTimeline" @attachmentRequested="onAttachmentRequested"
        />
      </div>
      <FriendsPanel v-else-if="view === 'friends'" :friends="friends" @changed="refreshAll" @open="viewFriend" />
      <ChannelsPanel v-else-if="view === 'channels'" :friends="friends" />
      <div v-else-if="view === 'public'" class="public-page">
        <div class="row">
          <button class="primary" @click="requestPublic">{{ t("requestPublicContent") }}</button>
          <button @click="refreshPublicTimeline">{{ t("refresh") }}</button>
        </div>
        <div v-if="publicTimeline.length === 0" class="empty">{{ t("emptyPublicTimeline") }}</div>
        <PostCard
          v-for="p in publicTimeline"
          :key="p.id"
          :item="p"
          :own="own"
          @open="openThreadWithData"
          @reacted="refreshPublicTimeline" @attachmentRequested="onAttachmentRequested"
        />
      </div>
      <SettingsPanel v-else :own="own" @saved="refreshAll" />
    </main>

    <!-- File receive confirm modal -->
    <div v-if="currentFileRequest" class="modal-overlay">
      <div class="modal">
        <h3>{{ t("fileRequestTitle") }}</h3>
        <p>
          <strong>{{ currentFileRequest.friendName || `#${currentFileRequest.friendNumber}` }}</strong>
          {{ t("fileRequestFrom") }}
        </p>
        <div class="file-req-info">
          <div>{{ currentFileRequest.filename }}</div>
          <div class="mono">{{ formatFileSize(currentFileRequest.fileSize) }}</div>
        </div>
        <div class="row">
          <button @click="rejectFileRequest">{{ t("fileReject") }}</button>
          <button class="primary" @click="acceptFileRequest">{{ t("fileAccept") }}</button>
        </div>
      </div>
    </div>

    <!-- Add friend / search users modal -->
    <div v-if="showAddFriend" class="modal-overlay" @click.self="showAddFriend = false">
      <div class="modal">
        <h3>{{ t("searchUsers") }}</h3>
        <input v-model="addToxid" class="mono" :placeholder="t('searchUserPlaceholder')" @keydown.enter.prevent="submitAddFriend" />
        <p v-if="addError" class="error">{{ addError }}</p>
        <div v-if="searchingUsers" class="empty">{{ t("searchingUsers") }}</div>
        <div v-for="d in userSearchResults" :key="d.pubkey" class="search-result">
          <span>{{ d.name || t("unnamed") }}</span>
          <span class="mono">{{ d.pubkey.slice(0, 12) }}…</span>
          <button @click="viewSearchedUser(d)">{{ t("viewProfile") }}</button>
        </div>
        <div class="row">
          <button @click="showAddFriend = false">{{ t("cancel") }}</button>
          <button class="primary" :disabled="addBusy || !addToxid.trim()" @click="submitAddFriend">
            {{ addBusy ? t("processing") : t("search") }}
          </button>
        </div>
      </div>
    </div>

    <!-- Right: me panel -->
    <aside class="me-panel" v-if="own">
      <div class="me-card">
        <Avatar :src="own.avatar" :name="own.name" :size="64" />
        <div class="me-name">{{ own.name || t("noNickname") }}</div>
        <div class="mono">{{ own.pubkey.slice(0, 20) }}…</div>
        <div class="me-stats">
          <span>{{ t("friendCount", { count: own.friendCount }) }}</span>
        </div>
      </div>
    </aside>

    <!-- Attachment/file transfer status -->
    <div v-if="transfers.length > 0" class="transfer-panel">
      <div class="transfer-head">📤 {{ t("transfers") }}</div>
      <div v-for="(tr, i) in transfers" :key="`${tr.direction}-${tr.friendNumber}-${tr.fileNumber}`" class="transfer-item">
        <span class="transfer-name" :title="tr.filename">
          {{ tr.direction === "send" ? "↑" : "↓" }} {{ tr.filename }}
        </span>
        <div class="progress">
          <div class="progress-bar" :style="{ width: transferPct(tr) + '%' }"></div>
        </div>
        <span class="transfer-pct">{{ transferPct(tr) }}%</span>
      </div>
    </div>
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
  text-align: center;
  justify-content: center;
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

.relay-warning {
  margin-top: auto;
  padding: 8px 10px;
  font-size: 12px;
  color: var(--danger);
  background: var(--bg-3);
  border: 1px solid var(--border);
  border-radius: 8px;
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
.public-page {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.public-page .row {
  display: flex;
  gap: 8px;
}
.profile-page {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.profile-card {
  display: flex;
  align-items: center;
  gap: 12px;
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 14px;
}
.profile-name {
  font-size: 16px;
  font-weight: 700;
}
.follow-btn {
  margin-left: auto;
}
.followed-tag {
  margin-left: auto;
  color: var(--text-dim);
  font-size: 13px;
}
.transfer-panel {
  position: fixed;
  right: 16px;
  bottom: 16px;
  width: 320px;
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px 12px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  z-index: 50;
}
.transfer-head {
  font-weight: 600;
  font-size: 13px;
  margin-bottom: 6px;
}
.transfer-item {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  padding: 4px 0;
}
.transfer-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.transfer-pct {
  color: var(--text-dim);
  min-width: 36px;
  text-align: right;
}
.progress {
  width: 90px;
  height: 6px;
  background: var(--bg-3);
  border-radius: 999px;
  overflow: hidden;
}
.progress-bar {
  height: 100%;
  background: var(--accent);
  border-radius: 999px;
  transition: width 0.3s;
}
.profile-bio {
  margin-top: 4px;
  font-size: 13px;
  color: var(--text-dim);
  white-space: pre-wrap;
  word-break: break-word;
}
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}
.modal {
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 18px;
  width: 420px;
  max-width: 90vw;
  display: flex;
  flex-direction: column;
  gap: 10px;
  box-shadow: var(--shadow);
}
.modal h3 {
  margin: 0;
}
.modal .row {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.modal .error {
  color: var(--danger);
  font-size: 12px;
}
.search-result {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
}
.file-req-info {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px 10px;
  display: flex;
  justify-content: space-between;
  gap: 8px;
  word-break: break-all;
}
.search-result span:first-child {
  flex: 1;
  font-weight: 600;
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
