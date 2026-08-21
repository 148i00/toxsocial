<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { api, onEvent } from "../api";
import { channelMessages, clearChannelMessages, mergeChannelHistory, pushChannelMessage } from "../channelStore";
import { t } from "../i18n";
import type { ConferencePeerInfo, FriendInfo, PublicChannelInfo } from "../types";

const props = defineProps<{ friends: FriendInfo[] }>();

const ME_PEER = "我";
const conferenceNumber = ref<number | null>(null);
const channelId = ref("");
const isCurrentChannelOwned = ref(false);
const peerCount = ref(0);
const ownToxid = ref("");
const myChannels = ref<{ name: string; conferenceNumber: number }[]>([]);
const subName = ref("");
const friendNumber = ref("0");
const inviteToxid = ref("");
const message = ref("");
const log = ref<string[]>([]);
const MAX_LOG = 200;
function pushLog(text: string) {
  log.value.push(text);
  if (log.value.length > MAX_LOG) log.value.splice(0, log.value.length - MAX_LOG);
}
const busy = ref(false);
const error = ref("");
const peers = ref<ConferencePeerInfo[]>([]);
const joiningChannelId = ref("");
const requestedChannels = ref<string[]>([]);
const currentChannelName = computed(() => {
  const n = conferenceNumber.value;
  if (n === null) return "";
  return myChannels.value.find((c) => c.conferenceNumber === n)?.name || t("channelNameWithNumber", { number: n });
});
const currentMessages = computed(() =>
  channelMessages.filter((m) => {
    // Prefer the stable channel id: toxcore reuses conference numbers after
    // deletion, so matching by number alone would leak the old channel's
    // messages into a new channel on the same number. BUT the channel id is
    // only known after `refreshChannelId` succeeds (the conference may not
    // be ready yet right after joining); while it is unknown, fall back to
    // the conference number so the chat window is not blank.
    if (channelId.value !== "") {
      return (
        (m.channelId !== undefined && m.channelId === channelId.value) ||
        (m.channelId === undefined && m.conferenceNumber === conferenceNumber.value)
      );
    }
    return m.conferenceNumber === conferenceNumber.value;
  }),
);
const chatMessagesRef = ref<HTMLElement | null>(null);
const showManage = ref(false);
watch(currentMessages, async () => {
  await nextTick();
  chatMessagesRef.value?.scrollTo({ top: chatMessagesRef.value.scrollHeight });
});

const publicChannels = ref<PublicChannelInfo[]>([]);
const channelName = ref("");
const channelDesc = ref("");
const newChannelName = ref("");
const hostInput = ref("");

const currentChannelPublic = computed(() =>
  publicChannels.value.find((ch) => ch.channelId === channelId.value) || null,
);
const canPublishChannel = computed(() => {
  const pub = currentChannelPublic.value;
  if (pub) {
    return (pub.hosts || []).some((h) =>
      h === ownToxid.value || h === ownToxid.value.slice(0, 64) ||
      ownToxid.value.startsWith(h) || h.startsWith(ownToxid.value.slice(0, 64))
    );
  }
  return isCurrentChannelOwned.value;
});

async function loadPublicChannels() {
  try {
    publicChannels.value = await api.listPublicChannels();
  } catch (e) {
    publicChannels.value = [];
    pushLog(t("loadPublicChannelsFailed", { error: String(e) }));
  }
  // 定期向 Relay 上报“我在哪些公共频道”，让新成员可以找任意在线成员拉入。
  try {
    await api.reportChannelMemberships();
  } catch {
    // relay 上报失败不影响频道页使用
  }
}

function saveChannelNames() {
  try {
    const saved = JSON.parse(localStorage.getItem("toxsocial_channel_names") || "{}");
    for (const c of myChannels.value) {
      saved[String(c.conferenceNumber)] = c.name;
    }
    localStorage.setItem("toxsocial_channel_names", JSON.stringify(saved));
  } catch {
    // ignore localStorage errors
  }
}

function ensureMyChannel(n: number, name?: string) {
  if (!myChannels.value.some((c) => c.conferenceNumber === n)) {
    myChannels.value.push({ name: name || t("channelNameWithNumber", { number: n }), conferenceNumber: n });
    saveChannelNames();
  }
}

/** Stable ids of channels we are (still) in — survives restarts because the
 * tox conference list is persisted. Used to show public channels as
 * "joined" even when they are not the currently selected channel. */
const joinedChannelIds = ref<string[]>([]);

async function loadMyChannels() {
  try {
    const nums = await api.listConferences();
    const saved = JSON.parse(localStorage.getItem("toxsocial_channel_names") || "{}");
    const merged: { name: string; conferenceNumber: number }[] = [];
    const ids: string[] = [];
    for (const n of nums) {
      const existing = myChannels.value.find((c) => c.conferenceNumber === n);
      let name = existing?.name || "";
      if (!name) {
        name = saved[String(n)] || t("channelNameWithNumber", { number: n });
      }
      merged.push({ name, conferenceNumber: n });
      const id = await api.getConferenceId(n).catch(() => "");
      if (id) ids.push(id);
    }
    myChannels.value = merged;
    joinedChannelIds.value = ids;
    saveChannelNames();
    if (conferenceNumber.value === null && nums.length > 0) {
      await switchChannel(nums[0]);
    }
  } catch (e) {
    pushLog(t("loadMyChannelsFailed", { error: String(e) }));
  }
}

async function deletePublic(ch: PublicChannelInfo) {
  if (!confirm(t("confirmDeletePublicChannel", { name: ch.name }))) return;
  try {
    await api.deletePublicChannel(ch.channelId);
    await loadPublicChannels();
    pushLog(t("publicChannelDeleted", { name: ch.name }));
  } catch (e) {
    pushLog(t("deleteFailed", { error: String(e) }));
  }
}

async function addHost(ch: PublicChannelInfo) {
  const toxid = hostInput.value.trim();
  if (!toxid) return;
  if (toxid.length !== 64 && toxid.length !== 76) {
    pushLog(t("invalidToxidLength"));
    return;
  }
  try {
    await api.addChannelHost(ch.channelId, toxid);
    hostInput.value = "";
    await loadPublicChannels();
    pushLog(t("hostAdded", { toxid: toxid.slice(0, 8) }));
  } catch (e) {
    pushLog(t("addHostFailed", { error: String(e) }));
  }
}

async function removeHost(ch: PublicChannelInfo, toxid: string) {
  if (!confirm(t("confirmRemoveHost", { toxid: toxid.slice(0, 8) }))) return;
  try {
    await api.removeChannelHost(ch.channelId, toxid);
    await loadPublicChannels();
    pushLog(t("hostRemoved", { toxid: toxid.slice(0, 8) }));
  } catch (e) {
    pushLog(t("removeHostFailed", { error: String(e) }));
  }
}

async function enterOwnChannel(targetChannelId: string) {
  if (channelId.value === targetChannelId) {
    pushLog(t("alreadyInChannel"));
    return;
  }
  try {
    const confs = await api.listConferences();
    for (const n of confs) {
      const id = await api.getConferenceId(n);
      if (id === targetChannelId) {
        await switchChannel(n);
        pushLog(t("enteredOwnChannel"));
        return;
      }
    }
    pushLog(t("ownChannelNotFound"));
  } catch (e) {
    pushLog(t("enterOwnChannelFailed", { error: String(e) }));
  }
}

function isOwnChannel(ch: PublicChannelInfo): boolean {
  const host = ch.hostToxid ? String(ch.hostToxid) : "";
  const me = ownToxid.value ? String(ownToxid.value) : "";
  const myPub = me.slice(0, 64);
  if (!host || !me) return false;
  return host === me || host === myPub || host.startsWith(myPub) || me.startsWith(host.slice(0, 64));
}

function channelContacts(ch: PublicChannelInfo): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  const add = (t?: string) => {
    if (!t || seen.has(t)) return;
    seen.add(t);
    out.push(t);
  };
  add(ch.hostToxid);
  for (const h of ch.hosts || []) add(h);
  for (const m of ch.members || []) add(m);
  return out;
}

function isFriendToxid(t: string): boolean {
  return props.friends.some(
    (f) => f.toxid === t || f.pubkey === t || f.toxid.startsWith(t.slice(0, 64)) || t.startsWith(f.pubkey),
  );
}

async function tryJoinViaContact(ch: PublicChannelInfo, contact: string): Promise<boolean> {
  try {
    if (isFriendToxid(contact)) {
      await api.sendJoinChannel(contact, ch.channelId);
      if (!requestedChannels.value.includes(ch.channelId)) requestedChannels.value.push(ch.channelId);
      pushLog(t("joinRequestSentToMember", { name: ch.name, contact: contact.slice(0, 8) }));
      return true;
    }
    await api.addFriend(contact, `join_channel ${ch.channelId}`);
    if (!requestedChannels.value.includes(ch.channelId)) requestedChannels.value.push(ch.channelId);
    pushLog(t("joinRequestSentAsFriend", { name: ch.name, contact: contact.slice(0, 8) }));
    return true;
  } catch (e) {
    const msg = String(e);
    if (msg.includes("已发送") || msg.includes("already")) {
      if (!requestedChannels.value.includes(ch.channelId)) requestedChannels.value.push(ch.channelId);
      pushLog(t("joinRequestAlreadySent", { name: ch.name, contact: contact.slice(0, 8) }));
      return true;
    }
    return false;
  }
}

async function joinPublic(ch: PublicChannelInfo) {
  if (joiningChannelId.value) return;
  joiningChannelId.value = ch.channelId;
  try {
    if (isOwnChannel(ch) || isJoined(ch)) {
      await enterOwnChannel(ch.channelId);
      return;
    }
    const contacts = channelContacts(ch);
    if (contacts.length === 0) {
      pushLog(t("channelNoContacts", { name: ch.name }));
      return;
    }
    for (const contact of contacts) {
      const ok = await tryJoinViaContact(ch, contact);
      if (ok) return;
    }
    pushLog(t("joinChannelFailed", { name: ch.name }));
  } catch (e) {
    pushLog(t("joinChannelError", { name: ch.name, error: String(e) }));
  } finally {
    joiningChannelId.value = "";
  }
}

function isChannelActive(ch: PublicChannelInfo): boolean {
  return !!ch.channelId && ch.channelId === channelId.value;
}

/** We are actually in this public channel (its conference is in our
 * persisted list), so it should show as joined after restarts too. */
function isJoined(ch: PublicChannelInfo): boolean {
  return !!ch.channelId && joinedChannelIds.value.includes(ch.channelId);
}

function isRequested(ch: PublicChannelInfo): boolean {
  return requestedChannels.value.includes(ch.channelId);
}

async function copyPublicInvite(ch: PublicChannelInfo) {
  const host = ch.hostToxid || "";
  if (!host || !ch.channelId) {
    pushLog(t("noInviteInfo", { name: ch.name }));
    return;
  }
  const text = `${t("inviteToxidLabel")}: ${host}
${t("inviteChannelIdLabel")}: ${ch.channelId}
${t("inviteJoinMessageLabel")}: join_channel ${ch.channelId}`;
  try {
    await navigator.clipboard.writeText(text);
    pushLog(t("inviteCopied", { name: ch.name }));
  } catch {
    pushLog(t("copyFailed"));
  }
}

async function publishChannel() {
  if (conferenceNumber.value === null || !channelName.value.trim()) {
    error.value = t("channelNameRequired");
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    await api.registerPublicChannel(
      conferenceNumber.value,
      channelName.value.trim(),
      channelDesc.value.trim(),
    );
    pushLog(t("channelPublished", { name: channelName.value.trim() }));
    channelName.value = "";
    channelDesc.value = "";
    await loadPublicChannels();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function pasteInvite() {
  try {
    const text = await navigator.clipboard.readText();
    const toxid = text.match(/ToxID:\s*(\S+)/i)?.[1] || "";
    const ch = text.match(/(?:频道ID|频道 ID|Channel ID|ChannelID)\s*:\s*(\S+)/i)?.[1] || "";
    if (!toxid || !ch) {
      pushLog(t("invalidInvite"));
      return;
    }
    await api.addFriend(toxid, `join_channel ${ch}`);
    pushLog(t("invitePasted"));
  } catch (e) {
    pushLog(t("pasteFailed", { error: String(e) }));
  }
}

async function refreshChannelId() {
  if (conferenceNumber.value === null) return;
  try {
    channelId.value = await api.getConferenceId(conferenceNumber.value);
  } catch {
    channelId.value = "";
  }
}

async function refreshPeerCount() {
  if (conferenceNumber.value === null) return;
  try {
    peerCount.value = await api.getConferencePeerCount(conferenceNumber.value);
  } catch {
    peerCount.value = 0;
  }
}

async function loadPeers() {
  if (conferenceNumber.value === null) return;
  try {
    peers.value = await api.conferencePeers(conferenceNumber.value);
  } catch {
    peers.value = [];
  }
}

async function deleteChannel(n: number) {
  const target = myChannels.value.find((c) => c.conferenceNumber === n);
  if (!confirm(t("confirmDeleteChannel", { name: target?.name || n }))) return;
  busy.value = true;
  error.value = "";
  try {
    // Remember the channel id before deletion so we can drop its buffered
    // messages; toxcore will reuse the number for the next channel.
    const deletedChannelId = await api.getConferenceId(n).catch(() => "");
    await api.conferenceDelete(n);
    if (deletedChannelId) {
      clearChannelMessages(deletedChannelId, n);
      joinedChannelIds.value = joinedChannelIds.value.filter((id) => id !== deletedChannelId);
    }
    myChannels.value = myChannels.value.filter((c) => c.conferenceNumber !== n);
    saveChannelNames();
    if (conferenceNumber.value === n) {
      conferenceNumber.value = null;
      channelId.value = "";
      peerCount.value = 0;
      peers.value = [];
      if (myChannels.value.length > 0) {
        await switchChannel(myChannels.value[0].conferenceNumber);
      }
    }
    pushLog(t("channelDeleted", { number: n }));
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function createSubChannel() {
  if (!subName.value.trim() || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const n = await api.conferenceNew();
    const name = subName.value.trim();
    myChannels.value.push({ name, conferenceNumber: n });
    saveChannelNames();
    subName.value = "";
    await switchChannel(n);
    pushLog(t("channelCreated", { name, number: n }));
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function switchChannel(n: number) {
  conferenceNumber.value = n;
  await refreshChannelId();
  await refreshPeerCount();
  await loadPeers();
  await loadHistory(n);
  isCurrentChannelOwned.value = await api.isChannelOwned(n).catch(() => false);
}

/** Retry fetching the stable channel id until it succeeds (the conference
 * may not be ready right after joining). */
async function refreshChannelIdWithRetry(n: number, attempts = 5) {
  for (let i = 0; i < attempts; i++) {
    try {
      const id = await api.getConferenceId(n);
      if (id) {
        channelId.value = id;
        return true;
      }
    } catch {
      /* not ready yet */
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  return false;
}

/** Load persisted chat history for this channel (survives restarts). */
async function loadHistory(n: number) {
  try {
    const msgs = await api.channelMessages(n, 300);
    if (msgs.length === 0) return;
    const name =
      myChannels.value.find((c) => c.conferenceNumber === n)?.name ||
      t("channelNameWithNumber", { number: n });
    const added = mergeChannelHistory(
      msgs.map((m) => ({
        id: m.id,
        conferenceNumber: n,
        channelId: channelId.value || undefined,
        channelName: name,
        peer: m.direction === 1 ? ME_PEER : m.peerName || `#${n}`,
        text: m.text,
        ts: m.ts,
      })),
    );
    if (added > 0) {
      await nextTick();
      chatMessagesRef.value?.scrollTo({ top: chatMessagesRef.value.scrollHeight });
    }
  } catch {
    // history load failure is non-fatal
  }
}

async function create() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const n = await api.conferenceNew();
    const name = newChannelName.value.trim() || t("unnamedChannel");
    myChannels.value.push({ name, conferenceNumber: n });
    saveChannelNames();
    newChannelName.value = "";
    conferenceNumber.value = n;
    await refreshChannelId();
    await refreshPeerCount();
    await loadPeers();
    pushLog(t("channelCreated", { name, number: n }));
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function inviteByToxid() {
  if (conferenceNumber.value === null || !inviteToxid.value.trim() || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await api.conferenceInviteByToxid(conferenceNumber.value, inviteToxid.value.trim());
    pushLog(t("invitedByToxid", { toxid: inviteToxid.value.slice(0, 8), number: conferenceNumber.value }));
    inviteToxid.value = "";
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function copyInvite() {
  if (!ownToxid.value || !channelId.value) return;
  const text = `${t("inviteToxidLabel")}: ${ownToxid.value}
${t("inviteChannelIdLabel")}: ${channelId.value}
${t("inviteJoinMessageLabel")}: join_channel ${channelId.value}`;
  try {
    await navigator.clipboard.writeText(text);
    pushLog(t("inviteInfoCopied"));
  } catch {
    pushLog(t("copyFailed"));
  }
}

async function invite() {
  if (conferenceNumber.value === null || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await api.conferenceInvite(Number(friendNumber.value), conferenceNumber.value);
    pushLog(t("invitedFriendNumber", { friendNumber: friendNumber.value, number: conferenceNumber.value }));
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function send() {
  if (conferenceNumber.value === null || !message.value.trim() || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const result = await api.conferenceSend(conferenceNumber.value, message.value.trim());
    const name = myChannels.value.find((c) => c.conferenceNumber === conferenceNumber.value)?.name || t("channelNameWithNumber", { number: conferenceNumber.value });
    pushChannelMessage({ id: result.id, conferenceNumber: conferenceNumber.value, channelId: channelId.value || undefined, channelName: name, peer: ME_PEER, text: message.value.trim(), ts: Date.now() });
    message.value = "";
    if (result.queued) {
      pushLog(t("channelQueuedOffline"));
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

let publicTimer: ReturnType<typeof setInterval> | undefined;

onMounted(async () => {
  try {
    const info = await api.getOwnInfo();
    ownToxid.value = info.toxid;
  } catch {
    /* ignore */
  }
  await loadMyChannels();
  await loadPublicChannels();
  publicTimer = setInterval(() => {
    loadPublicChannels();
  }, 15_000);
  onEvent("channel:connected", async (e: { conferenceNumber: number }) => {
    ensureMyChannel(e.conferenceNumber);
    await switchChannel(e.conferenceNumber);
    // The conference is ready now; make sure the stable channel id is known
    // (it may have failed right after joining) and reload history under it.
    if (await refreshChannelIdWithRetry(e.conferenceNumber, 5)) {
      await loadHistory(e.conferenceNumber);
      if (!joinedChannelIds.value.includes(channelId.value)) {
        joinedChannelIds.value.push(channelId.value);
      }
    }
    requestedChannels.value = requestedChannels.value.filter((id) => id !== channelId.value);
    pushLog(t("channelConnectedNumber", { number: e.conferenceNumber }));
  });
  onEvent("channel:joined", async (e: { conferenceNumber: number; friendNumber: number }) => {
    ensureMyChannel(e.conferenceNumber);
    await switchChannel(e.conferenceNumber);
    if (await refreshChannelIdWithRetry(e.conferenceNumber, 5)) {
      await loadHistory(e.conferenceNumber);
      if (!joinedChannelIds.value.includes(channelId.value)) {
        joinedChannelIds.value.push(channelId.value);
      }
    }
    requestedChannels.value = requestedChannels.value.filter((id) => id !== channelId.value);
    pushLog(t("joinedViaInvite", { friendNumber: e.friendNumber, number: e.conferenceNumber }));
  });
  onEvent("channel:peer_list_changed", async () => {
    await refreshPeerCount();
    await loadPeers();
  });
});

onBeforeUnmount(() => {
  if (publicTimer) clearInterval(publicTimer);
});
</script>

<template>
  <div class="channels-layout">
    <!-- Left: vertical channel list -->
    <div class="channels-sidebar">
      <div class="sidebar-header">
        <span class="sidebar-title">{{ t("channelsTitle") }}</span>
        <button class="primary small" :disabled="busy" @click="create">{{ t("createJoin") }}</button>
      </div>
      <div class="sidebar-create-row">
        <input v-model="newChannelName" :placeholder="t('newChannelNamePlaceholder')" @keydown.enter="create" />
      </div>

      <div class="channel-list">
        <div v-if="myChannels.length === 0" class="empty">{{ t("noChannels") }}</div>
        <div
          v-for="sub in myChannels"
          :key="sub.conferenceNumber"
          class="channel-item"
          :class="{ active: conferenceNumber === sub.conferenceNumber }"
          @click="switchChannel(sub.conferenceNumber)"
        >
          <div class="channel-item-info">
            <div class="channel-item-name">{{ sub.name }}</div>
            <div class="channel-item-desc">#{{ sub.conferenceNumber }}</div>
          </div>
          <button class="mini danger" :disabled="busy" @click.stop="deleteChannel(sub.conferenceNumber)">{{ t("deleteShort") }}</button>
        </div>
      </div>

      <details class="public-section">
        <summary>{{ t("publicChannels") }}</summary>
        <div v-if="publicChannels.length === 0" class="empty">{{ t("noPublicChannels") }}</div>
        <div v-for="ch in publicChannels" :key="ch.channelId" class="public-item">
          <div class="public-item-info" @click="joinPublic(ch)">
            <div class="channel-item-name">
              {{ ch.name }}
              <span class="member-count" :title="t('onlineMembersTitle')">👥 {{ ch.members?.length ?? 0 }}</span>
            </div>
            <div class="channel-item-desc">{{ ch.desc || t("noDescription") }}</div>
          </div>
          <div class="public-item-actions">
            <button class="mini" :disabled="busy || joiningChannelId === ch.channelId || isChannelActive(ch) || isRequested(ch)" @click="joinPublic(ch)">
              {{ isChannelActive(ch) ? t("joined") : (isJoined(ch) ? t("enter") : (isRequested(ch) ? t("requested") : t("join"))) }}
            </button>
            <button class="mini" @click="copyPublicInvite(ch)">{{ t("copy") }}</button>
            <button v-if="ch.hosts && ch.hosts.includes(ownToxid)" class="mini danger" @click="deletePublic(ch)">{{ t("deleteShort") }}</button>
          </div>
          <details v-if="ch.hosts && ch.hosts.includes(ownToxid)" class="host-manage">
            <summary>{{ t("manageHosts") }}</summary>
            <div class="row">
              <input v-model="hostInput" :placeholder="t('addHostPlaceholder')" />
              <button class="mini" :disabled="busy || !hostInput.trim()" @click="addHost(ch)">{{ t("add") }}</button>
            </div>
            <div v-for="h in ch.hosts" :key="h" class="host-row">
              <span class="mono">{{ h.slice(0, 12) }}…</span>
              <button class="mini danger" :disabled="busy || h === ownToxid" @click="removeHost(ch, h)">{{ t("remove") }}</button>
            </div>
          </details>
        </div>
      </details>

      <div class="log-section">
        <div class="sidebar-title">{{ t("systemLog") }}</div>
        <div v-if="log.length === 0" class="empty">{{ t("noLogs") }}</div>
        <div v-for="(l, i) in log" :key="'l' + i" class="log-line">{{ l }}</div>
      </div>
    </div>

    <!-- Right: chat window -->
    <div class="chat-main">
      <template v-if="conferenceNumber !== null">
        <div class="chat-header">
          <div class="chat-title">
            <span class="chat-name">{{ currentChannelName || ('#' + conferenceNumber) }}</span>
            <span class="chat-meta">{{ t("peerCountLabel", { number: conferenceNumber, count: peerCount }) }}</span>
          </div>
          <div class="chat-actions">
            <button class="mini" @click="copyInvite">{{ t("copyInvite") }}</button>
            <button class="mini" @click="pasteInvite">{{ t("pasteJoin") }}</button>
            <button class="mini" @click="showManage = !showManage">{{ showManage ? t("hideManage") : t("manage") }}</button>
          </div>
        </div>

        <div ref="chatMessagesRef" class="chat-messages">
          <div v-if="currentMessages.length === 0" class="empty chat-empty">{{ t("noMessages") }}</div>
          <div v-for="(m, i) in currentMessages" :key="'m' + i" class="chat-msg" :class="{ mine: m.peer === ME_PEER }">
            <div class="bubble">
              <div class="bubble-peer">{{ m.peer === ME_PEER ? t('me') : m.peer }}</div>
              <div class="bubble-text">{{ m.text }}</div>
            </div>
          </div>
        </div>

        <div class="chat-composer">
          <textarea
            v-model="message"
            rows="2"
            maxlength="1372"
            :placeholder="t('channelInputPlaceholder')"
            @keydown.enter.exact.prevent="send"
          ></textarea>
          <button class="primary" :disabled="busy || !message.trim()" @click="send">{{ t("send") }}</button>
        </div>

        <div v-if="showManage" class="manage-panel">
          <div class="manage-grid">
            <div class="card">
              <div class="log-title">{{ t("inviteLinkChannelId") }}</div>
              <div class="mono toxid">{{ t("toxidLabel") }}: {{ ownToxid }}</div>
              <div class="mono toxid">{{ t("channelIdLabel") }}: {{ channelId }}</div>
              <div class="row">
                <button class="mini" @click="copyInvite">{{ t("copyInvite") }}</button>
                <button class="mini" @click="pasteInvite">{{ t("pasteJoin") }}</button>
              </div>
            </div>
            <div class="card">
              <div class="log-title">{{ t("inviteFriends") }}</div>
              <div class="row">
                <input v-model="inviteToxid" class="mono" :placeholder="t('friendToxidPlaceholder')" />
                <button :disabled="busy || !inviteToxid.trim()" @click="inviteByToxid">{{ t("invite") }}</button>
              </div>
              <div class="row">
                <input v-model="friendNumber" type="number" min="0" />
                <button :disabled="busy" @click="invite">{{ t("inviteFriendNumber") }}</button>
              </div>
            </div>
            <div v-if="canPublishChannel" class="card">
              <div class="log-title">{{ currentChannelPublic ? t("updatePublicChannel") : t("publishPublicChannel") }}</div>
              <input v-model="channelName" :placeholder="t('channelNameLabel')" />
              <input v-model="channelDesc" :placeholder="t('channelDescPlaceholder')" />
              <button class="primary" :disabled="busy || !channelName.trim()" @click="publishChannel">
                {{ currentChannelPublic ? t("update") : t("publish") }}
              </button>
            </div>
            <div class="card">
              <div class="log-title">{{ t("channelMembers") }}</div>
              <div v-if="peers.length === 0" class="empty">{{ t("noMembers") }}</div>
              <div v-for="p in peers" :key="p.peerNumber" class="member-row">
                <span>{{ p.name || t("unknownMember") }}</span>
                <span class="mono">{{ p.publicKey.slice(0, 10) }}…</span>
              </div>
            </div>
          </div>
        </div>
      </template>
      <div v-else class="chat-placeholder">
        <div class="empty">{{ t("selectChannelPrompt") }}</div>
      </div>
    </div>
  </div>
</template>


<style scoped>
.channels-layout {
  display: flex;
  gap: 12px;
  height: 100%;
  min-height: 600px;
}
.channels-sidebar {
  width: 260px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
  overflow-y: auto;
  max-height: 100%;
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px;
}
.sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.sidebar-title {
  font-weight: 700;
  font-size: 14px;
}
.sidebar-create-row {
  display: flex;
}
.sidebar-create-row input {
  flex: 1;
}
.channel-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.channel-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 8px;
  cursor: pointer;
  border: 1px solid transparent;
}
.channel-item:hover {
  background: var(--bg-3);
}
.channel-item.active {
  background: var(--bg-3);
  border-color: var(--accent);
}
.channel-item-info {
  flex: 1;
  min-width: 0;
}
.channel-item-name {
  font-weight: 600;
  font-size: 13px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.member-count {
  font-weight: 400;
  font-size: 11px;
  color: var(--text-dim);
  margin-left: 6px;
}
.channel-item-desc {
  font-size: 11px;
  color: var(--text-dim);
}
.public-section {
  border-top: 1px solid var(--border);
  padding-top: 8px;
  font-size: 13px;
}
.public-section summary {
  cursor: pointer;
  font-weight: 600;
  margin-bottom: 6px;
}
.public-item {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 6px 8px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 6px;
}
.public-item-info {
  cursor: pointer;
}
.public-item-actions {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}
.host-manage {
  margin-top: 4px;
  font-size: 12px;
}
.host-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  padding: 2px 0;
}
.log-section {
  border-top: 1px solid var(--border);
  padding-top: 8px;
  max-height: 180px;
  overflow-y: auto;
}
.log-line {
  font-size: 11px;
  color: var(--text-dim);
  padding: 2px 0;
  border-bottom: 1px dashed var(--border);
}
.chat-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  height: 100%;
  overflow: hidden;
}
.chat-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 12px 14px;
  border-bottom: 1px solid var(--border);
}
.chat-title {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.chat-name {
  font-size: 16px;
  font-weight: 700;
}
.chat-meta {
  font-size: 12px;
  color: var(--text-dim);
}
.chat-actions {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}
.chat-messages {
  flex: 1;
  overflow-y: auto;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
}
.chat-empty {
  text-align: center;
  padding: 40px 0;
}
.chat-msg {
  display: flex;
}
.chat-msg.mine {
  justify-content: flex-end;
}
.bubble {
  max-width: 70%;
  background: var(--bg-3);
  border: 1px solid var(--border);
  border-radius: 12px;
  padding: 8px 12px;
}
.chat-msg.mine .bubble {
  background: var(--accent);
  color: #fff;
  border-color: var(--accent);
}
.bubble-peer {
  font-size: 11px;
  opacity: 0.8;
  margin-bottom: 2px;
}
.bubble-text {
  white-space: pre-wrap;
  word-break: break-word;
  font-size: 14px;
  line-height: 1.5;
}
.chat-composer {
  display: flex;
  gap: 8px;
  padding: 10px 14px;
  border-top: 1px solid var(--border);
  align-items: flex-end;
}
.chat-composer textarea {
  flex: 1;
  min-height: 42px;
  resize: vertical;
}
.manage-panel {
  border-top: 1px solid var(--border);
  padding: 10px 14px;
  max-height: 40%;
  overflow-y: auto;
}
.manage-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 10px;
}
.chat-placeholder {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}
.member-row {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 12px;
  padding: 4px 0;
  border-bottom: 1px dashed var(--border);
}
.row {
  display: flex;
  gap: 6px;
  align-items: center;
}
.card {
  background: var(--bg-3);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.empty {
  color: var(--text-dim);
  font-size: 12px;
}
.mono {
  font-family: monospace;
}
.mini {
  padding: 2px 8px;
  font-size: 11px;
}
.danger {
  color: var(--danger);
}
.small {
  padding: 4px 10px;
  font-size: 12px;
}
.toxid {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 6px 8px;
  word-break: break-all;
  font-size: 12px;
}
</style>

