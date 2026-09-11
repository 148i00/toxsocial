<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { api, onEvent } from "../api";
import { t } from "../i18n";
import { renderMarkdown } from "../markdown";
import PostCard from "./PostCard.vue";
import PostComposer from "./PostComposer.vue";
import type { CommunityInfo, PublicChannelInfo, TimelineItem } from "../types";

const props = defineProps<{
  friends: { pubkey: string }[];
  own: import("../types").OwnInfo | null;
  /** Community picked from the sidebar list or a search hit. */
  openId?: string;
}>();
const emit = defineEmits<{ open: [id: string]; author: [pubkey: string] }>();

const myCommunities = ref<CommunityInfo[]>([]);
const publicDiscover = ref<PublicChannelInfo[]>([]);
const selected = ref<CommunityInfo | null>(null);
const feed = ref<TimelineItem[]>([]);
const feedLoading = ref(false);
const feedHasMore = ref(false);
const feedLoadingMore = ref(false);
const feedSentinel = ref<HTMLElement | null>(null);
let feedObserver: IntersectionObserver | null = null;
const newName = ref("");
const newDesc = ref("");
const busy = ref(false);
const error = ref("");
const joiningId = ref("");
let feedTimer: ReturnType<typeof setInterval> | undefined;

const selectedName = computed(
  () =>
    myCommunities.value.find((c) => c.channelId === selected.value?.channelId)?.name ||
    publicDiscover.value.find((c) => c.channelId === selected.value?.channelId)?.name ||
    selected.value?.name ||
    "",
);

async function loadMy() {
  try {
    myCommunities.value = await api.myCommunities();
    const want = props.openId;
    const target = want ? myCommunities.value.find((c) => c.channelId === want) : undefined;
    if (target && target.channelId !== selected.value?.channelId) {
      await select(target);
    } else if (!selected.value && myCommunities.value.length > 0) {
      await select(myCommunities.value[0]);
    }
  } catch {
    /* ignore */
  }
}

// A sidebar click or search hit changes openId without remounting the panel.
watch(
  () => props.openId,
  async (id) => {
    if (!id) return;
    const target = myCommunities.value.find((c) => c.channelId === id);
    if (target && target.channelId !== selected.value?.channelId) await select(target);
  },
);

async function loadDiscover() {
  try {
    const mine = new Set(myCommunities.value.map((c) => c.channelId));
    publicDiscover.value = (await api.listPublicChannels()).filter((c) => !mine.has(c.channelId));
  } catch {
    publicDiscover.value = [];
  }
}

async function select(c: CommunityInfo) {
  selected.value = c;
  feed.value = [];
  feedHasMore.value = false;
  await loadFeed();
  startFeedTimer();
}

async function loadFeed() {
  if (!selected.value) return;
  feedLoading.value = true;
  try {
    // Local cache first, then Relay aggregate for this community.
    await Promise.allSettled([
      api.fetchRelayPublicPosts(0, selected.value.channelId),
    ]);
    feed.value = await api.fetchCommunityTimeline(selected.value.channelId, 50);
    feedHasMore.value = feed.value.length >= 50;
    await nextTick();
    watchSentinel();
  } finally {
    feedLoading.value = false;
  }
}

function startFeedTimer() {
  if (feedTimer) clearInterval(feedTimer);
  feedTimer = setInterval(() => {
    if (!selected.value) return;
    api
      .fetchRelayPublicPosts(0, selected.value.channelId)
      .then(async () => {
        const older = await api.fetchCommunityTimeline(selected.value!.channelId, 50);
        const known = new Set(feed.value.map((p) => p.id));
        const fresh = older.filter((p) => !known.has(p.id));
        if (fresh.length > 0) feed.value = [...fresh, ...feed.value];
      })
      .catch(() => {
        /* relay may be unreachable */
      });
  }, 30_000);
}

async function loadMore() {
  if (!selected.value || feedLoadingMore.value || !feedHasMore.value || feed.value.length === 0) return;
  feedLoadingMore.value = true;
  try {
    const oldest = feed.value[feed.value.length - 1].ts;
    const older = await api.fetchCommunityTimeline(selected.value.channelId, 50, oldest);
    const known = new Set(feed.value.map((p) => p.id));
    feed.value.push(...older.filter((p) => !known.has(p.id)));
    feedHasMore.value = older.length >= 50;
  } finally {
    feedLoadingMore.value = false;
  }
}

function watchSentinel() {
  feedObserver?.disconnect();
  feedObserver = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) loadMore();
    },
    { rootMargin: "200px" },
  );
  if (feedSentinel.value) feedObserver.observe(feedSentinel.value);
}

async function createCommunity() {
  if (!newName.value.trim() || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const info = await api.createCommunity(newName.value.trim(), newDesc.value.trim());
    myCommunities.value.push(info);
    newName.value = "";
    newDesc.value = "";
    await select(info);
    await loadDiscover();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function joinCommunity(ch: PublicChannelInfo) {
  if (joiningId.value) return;
  joiningId.value = ch.channelId;
  try {
    await api.joinCommunity(ch.channelId, ch.name, ch.desc || "");
    await loadDiscover();
    await loadMy();
    pushLog(t("joinRequestSent", { name: ch.name }));
  } catch (e) {
    error.value = String(e);
  } finally {
    joiningId.value = "";
  }
}

const log = ref<string[]>([]);
function pushLog(text: string) {
  log.value.push(text);
  if (log.value.length > 100) log.value.splice(0, log.value.length - 100);
}

function onPosted() {
  loadFeed();
}

let unlistenPm: Awaited<ReturnType<typeof onEvent>> | undefined;
let heartbeatTimer: ReturnType<typeof setInterval> | undefined;

/** Resolve conference numbers for joined community conferences (the invite
 * may arrive after join_community recorded the channel id). */
async function refreshCommunityConferences() {
  let changed = false;
  try {
    const nums = await api.listConferences();
    for (const c of myCommunities.value) {
      if (c.conferenceNumber !== u32max()) continue;
      for (const n of nums) {
        const id = await api.getConferenceId(n).catch(() => "");
        if (id === c.channelId) {
          c.conferenceNumber = n;
          changed = true;
          break;
        }
      }
    }
    if (changed) {
      await api.updateCommunityConferences(
        myCommunities.value.map((c) => ({ channelId: c.channelId, conferenceNumber: c.conferenceNumber })),
      );
    }
  } catch {
    /* ignore */
  }
}

function u32max(): number {
  return 4294967295;
}

onMounted(async () => {
  await loadMy();
  await loadDiscover();
  // Heartbeat: report membership for joined community conferences so the
  // Relay's online-member count includes us while this page is open.
  heartbeatTimer = setInterval(() => {
    api.reportChannelMemberships().catch(() => {});
    refreshCommunityConferences();
  }, 15_000);
  // Community posts arrive as TSP envelopes inside community conferences and
  // are persisted with channel_id; a feed:post for the selected community
  // means a new post landed — refresh the feed.
  unlistenPm = await onEvent<{ id: string; author: string }>("feed:post", (e) => {
    if (viewIsActive()) {
      api.fetchCommunityTimeline(selected.value!.channelId, 50).then((list) => {
        if (!feed.value.some((p) => p.id === e.id)) {
          const known = new Set(feed.value.map((p) => p.id));
          feed.value = [...list.filter((p) => !known.has(p.id)), ...feed.value];
        }
      });
    }
  });
});

function viewIsActive(): boolean {
  return !!selected.value;
}

onBeforeUnmount(() => {
  if (feedTimer) clearInterval(feedTimer);
  if (heartbeatTimer) clearInterval(heartbeatTimer);
  feedObserver?.disconnect();
  unlistenPm?.();
});
</script>

<template>
  <div class="communities-layout">
    <!-- Left: community list -->
    <div class="communities-sidebar">
      <div class="sidebar-header">
        <span class="sidebar-title">{{ t("communitiesTitle") }}</span>
      </div>
      <div class="community-create">
        <input v-model="newName" :placeholder="t('communityNamePlaceholder')" @keydown.enter="createCommunity" />
        <input v-model="newDesc" :placeholder="t('communityDescPlaceholder')" @keydown.enter="createCommunity" />
        <button class="primary small" :disabled="busy || !newName.trim()" @click="createCommunity">
          {{ busy ? t("processing") : t("create") }}
        </button>
      </div>

      <div class="community-list">
        <div v-if="myCommunities.length === 0" class="empty">{{ t("noCommunities") }}</div>
        <div
          v-for="c in myCommunities"
          :key="c.channelId"
          class="community-item"
          :class="{ active: selected?.channelId === c.channelId }"
          @click="select(c)"
        >
          <div class="community-item-name">{{ c.name }}</div>
          <div class="community-item-desc">{{ c.desc || t("noDescription") }}</div>
        </div>
      </div>

      <details class="discover-section">
        <summary>{{ t("discoverCommunities") }}</summary>
        <div v-if="publicDiscover.length === 0" class="empty">{{ t("noCommunitiesFound") }}</div>
        <div v-for="ch in publicDiscover" :key="ch.channelId" class="discover-item">
          <div>
            <div class="community-item-name">
              {{ ch.name }}
              <span class="member-count">👥 {{ ch.members?.length ?? 0 }}</span>
            </div>
          </div>
          <button class="mini" :disabled="joiningId === ch.channelId" @click="joinCommunity(ch)">
            {{ joiningId === ch.channelId ? t("requested") : t("join") }}
          </button>
        </div>
      </details>

      <div v-if="error" class="community-error">{{ error }}</div>
      <div class="log-section">
        <div class="sidebar-title">{{ t("systemLog") }}</div>
        <div v-for="(l, i) in log" :key="'l' + i" class="log-line">{{ l }}</div>
      </div>
    </div>

    <!-- Right: community post feed -->
    <div class="community-feed">
      <template v-if="selected">
        <div class="feed-header">
          <span class="feed-name">{{ selectedName }}</span>
          <span class="feed-meta">{{ t("communityFeedHint") }}</span>
        </div>
        <PostComposer
          :own="props.own"
          :community="selected.channelId"
          :community-name="selectedName"
          @posted="onPosted"
        />
        <div v-if="feedLoading" class="empty">{{ t("loadingPublic") }}</div>
        <div v-else-if="feed.length === 0" class="empty">{{ t("emptyCommunityFeed") }}</div>
        <PostCard
          v-for="p in feed"
          :key="p.id"
          :item="p"
          :own="props.own"
          @open="emit('open', p.id)"
          @reacted="loadFeed"
          @author="emit('author', p.author)"
        />
        <div v-if="feedHasMore" ref="feedSentinel" class="empty">
          {{ feedLoadingMore ? t("loadingPublic") : t("loadMore") }}
        </div>
      </template>
      <div v-else class="empty feed-empty">{{ t("selectCommunityPrompt") }}</div>
    </div>
  </div>
</template>

<style scoped>
.communities-layout {
  display: flex;
  height: 100%;
  min-height: 0;
}
.communities-sidebar {
  width: 260px;
  border-right: 1px solid var(--border);
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  overflow-y: auto;
}
.sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.sidebar-title {
  font-weight: 700;
}
.community-create {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.community-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.community-item {
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  cursor: pointer;
}
.community-item:hover,
.community-item.active {
  border-color: var(--accent);
}
.community-item-name {
  font-weight: 600;
  font-size: 13px;
}
.community-item-desc {
  color: var(--text-dim);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.member-count {
  font-weight: 400;
  font-size: 11px;
  color: var(--text-dim);
  margin-left: 6px;
}
.discover-section summary {
  cursor: pointer;
  color: var(--text-dim);
  font-size: 13px;
}
.discover-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 8px;
  border: 1px solid var(--border);
  border-radius: 8px;
  margin-bottom: 6px;
}
.community-error {
  color: var(--danger);
  font-size: 12px;
}
.community-feed {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: 12px 16px;
}
.feed-header {
  display: flex;
  align-items: baseline;
  gap: 10px;
  margin-bottom: 10px;
}
.feed-name {
  font-size: 17px;
  font-weight: 700;
}
.feed-meta {
  color: var(--text-dim);
  font-size: 12px;
}
.feed-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
}
</style>
