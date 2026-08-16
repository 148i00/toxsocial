<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { api, onEvent } from "../api";
import { t } from "../i18n";
import type { ConferencePeerInfo, FriendInfo, PublicChannelInfo } from "../types";

const props = defineProps<{ friends: FriendInfo[] }>();

const conferenceNumber = ref<number | null>(null);
const channelId = ref("");
const peerCount = ref(0);
const ownToxid = ref("");
const myChannels = ref<{ name: string; conferenceNumber: number }[]>([]);
const subName = ref("");
const friendNumber = ref("0");
const inviteToxid = ref("");
const message = ref("");
const messages = ref<{ conferenceNumber: number; channelName: string; peer: string; text: string }[]>([]);
const log = ref<string[]>([]);
const busy = ref(false);
const error = ref("");
const peers = ref<ConferencePeerInfo[]>([]);
const joiningChannelId = ref("");
const requestedChannels = ref<string[]>([]);
const currentChannelName = computed(() => {
  const n = conferenceNumber.value;
  if (n === null) return "";
  return myChannels.value.find((c) => c.conferenceNumber === n)?.name || `频道 #${n}`;
});
const currentMessages = computed(() =>
  messages.value.filter((m) => m.conferenceNumber === conferenceNumber.value),
);
const chatMessagesRef = ref<HTMLElement | null>(null);
watch(currentMessages, async () => {
  await nextTick();
  chatMessagesRef.value?.scrollTo({ top: chatMessagesRef.value.scrollHeight });
});

const publicChannels = ref<PublicChannelInfo[]>([]);
const channelName = ref("");
const channelDesc = ref("");
const newChannelName = ref("");
const hostInput = ref("");

async function loadPublicChannels() {
  try {
    publicChannels.value = await api.listPublicChannels();
  } catch (e) {
    publicChannels.value = [];
    log.value.push(`加载公共频道失败：${e}`);
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
    myChannels.value.push({ name: name || `频道 #${n}`, conferenceNumber: n });
    saveChannelNames();
  }
}

async function loadMyChannels() {
  try {
    const nums = await api.listConferences();
    const saved = JSON.parse(localStorage.getItem("toxsocial_channel_names") || "{}");
    const merged: { name: string; conferenceNumber: number }[] = [];
    for (const n of nums) {
      const existing = myChannels.value.find((c) => c.conferenceNumber === n);
      let name = existing?.name || "";
      if (!name) {
        name = saved[String(n)] || `频道 #${n}`;
      }
      merged.push({ name, conferenceNumber: n });
    }
    myChannels.value = merged;
    saveChannelNames();
    if (conferenceNumber.value === null && nums.length > 0) {
      await switchChannel(nums[0]);
    }
  } catch (e) {
    log.value.push(`加载我的频道失败：${e}`);
  }
}

async function deletePublic(ch: PublicChannelInfo) {
  if (!confirm(`删除公共频道「${ch.name}」？`)) return;
  try {
    await api.deletePublicChannel(ch.channelId);
    await loadPublicChannels();
    log.value.push(`已删除公共频道「${ch.name}」`);
  } catch (e) {
    log.value.push(`删除失败：${e}`);
  }
}

async function addHost(ch: PublicChannelInfo) {
  const toxid = hostInput.value.trim();
  if (!toxid) return;
  if (toxid.length !== 64 && toxid.length !== 76) {
    log.value.push("ToxID/公钥长度不正确（应为 64 位公钥或 76 位 ToxID）");
    return;
  }
  try {
    await api.addChannelHost(ch.channelId, toxid);
    hostInput.value = "";
    await loadPublicChannels();
    log.value.push(`已添加 host: ${toxid.slice(0, 8)}…`);
  } catch (e) {
    log.value.push(`添加 host 失败：${e}`);
  }
}

async function removeHost(ch: PublicChannelInfo, toxid: string) {
  if (!confirm(`移除 host ${toxid.slice(0, 8)}…？`)) return;
  try {
    await api.removeChannelHost(ch.channelId, toxid);
    await loadPublicChannels();
    log.value.push(`已移除 host: ${toxid.slice(0, 8)}…`);
  } catch (e) {
    log.value.push(`移除 host 失败：${e}`);
  }
}

async function enterOwnChannel(targetChannelId: string) {
  if (channelId.value === targetChannelId) {
    log.value.push("已经在这个频道里了");
    return;
  }
  try {
    const confs = await api.listConferences();
    for (const n of confs) {
      const id = await api.getConferenceId(n);
      if (id === targetChannelId) {
        await switchChannel(n);
        log.value.push("已进入自己创建的频道");
        return;
      }
    }
    log.value.push("没有找到自己的频道，请先在频道页创建/加入");
  } catch (e) {
    log.value.push(`进入自己频道失败：${e}`);
  }
}

function isOwnChannel(ch: PublicChannelInfo): boolean {
  const host = ch.hostToxid ? String(ch.hostToxid) : "";
  const me = ownToxid.value ? String(ownToxid.value) : "";
  const myPub = me.slice(0, 64);
  if (!host || !me) return false;
  return host === me || host === myPub || host.startsWith(myPub) || me.startsWith(host.slice(0, 64));
}

function isFriendHost(ch: PublicChannelInfo): boolean {
  const host = ch.hostToxid ? String(ch.hostToxid) : "";
  if (!host) return false;
  return props.friends.some(
    (f) => f.toxid === host || f.pubkey === host || f.toxid.startsWith(host.slice(0, 64)),
  );
}

async function joinPublic(ch: PublicChannelInfo) {
  if (joiningChannelId.value) return;
  joiningChannelId.value = ch.channelId;
  try {
    if (isOwnChannel(ch)) {
      await enterOwnChannel(ch.channelId);
      return;
    }
    if (!ch.hostToxid || ch.hostToxid.length < 70) {
      log.value.push(`「${ch.name}」暂时没有可用的 host，等待频道管理员接入。`);
      return;
    }
    if (isFriendHost(ch)) {
      try {
        await api.sendJoinChannel(ch.hostToxid, ch.channelId);
        if (!requestedChannels.value.includes(ch.channelId)) {
          requestedChannels.value.push(ch.channelId);
        }
        log.value.push(`已向「${ch.name}」host 发送加入申请。`);
      } catch (e) {
        log.value.push(`加入「${ch.name}」失败：${e}`);
      }
      return;
    }
    try {
      await api.addFriend(ch.hostToxid, `join_channel ${ch.channelId}`);
      if (!requestedChannels.value.includes(ch.channelId)) {
        requestedChannels.value.push(ch.channelId);
      }
      log.value.push(`已向「${ch.name}」host 发送好友请求/加入申请。`);
    } catch (e) {
      const msg = String(e);
      if (msg.includes("已发送") || msg.includes("already")) {
        if (!requestedChannels.value.includes(ch.channelId)) {
          requestedChannels.value.push(ch.channelId);
        }
        log.value.push(`加入「${ch.name}」：请求已发送，等待 host 接受。`);
      } else {
        log.value.push(`加入「${ch.name}」失败：${e}`);
      }
    }
  } catch (e) {
    log.value.push(`加入「${ch.name}」发生错误：${e}`);
  } finally {
    joiningChannelId.value = "";
  }
}

function isChannelActive(ch: PublicChannelInfo): boolean {
  return !!ch.channelId && ch.channelId === channelId.value;
}

function isRequested(ch: PublicChannelInfo): boolean {
  return requestedChannels.value.includes(ch.channelId);
}

async function copyPublicInvite(ch: PublicChannelInfo) {
  const host = ch.hostToxid || "";
  if (!host || !ch.channelId) {
    log.value.push(`「${ch.name}」暂时没有可用的邀请信息。`);
    return;
  }
  const text = `ToxID: ${host}
频道ID: ${ch.channelId}
好友请求附言: join_channel ${ch.channelId}`;
  try {
    await navigator.clipboard.writeText(text);
    log.value.push(`已复制「${ch.name}」邀请信息`);
  } catch {
    log.value.push("复制失败，请手动复制");
  }
}

async function publishChannel() {
  if (conferenceNumber.value === null || !channelName.value.trim()) {
    error.value = "请先创建/加入频道并填写频道名称";
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
    log.value.push(`已把频道「${channelName.value.trim()}」发布为公共频道`);
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
    const toxid = text.match(/ToxID:\s*(\S+)/)?.[1] || "";
    const ch = text.match(/频道ID:\s*(\S+)/)?.[1] || "";
    if (!toxid || !ch) {
      log.value.push("剪贴板里没有找到有效的频道邀请信息");
      return;
    }
    await api.addFriend(toxid, `join_channel ${ch}`);
    log.value.push("已粘贴邀请并发送加入申请");
  } catch (e) {
    log.value.push(`粘贴失败：${e}`);
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
  if (!confirm(`删除频道「${target?.name || n}」？`)) return;
  busy.value = true;
  error.value = "";
  try {
    await api.conferenceDelete(n);
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
    log.value.push(`已删除频道 #${n}`);
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
    log.value.push(`已创建频道「${name}」#${n}`);
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
}

async function create() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const n = await api.conferenceNew();
    const name = newChannelName.value.trim() || "未命名频道";
    myChannels.value.push({ name, conferenceNumber: n });
    saveChannelNames();
    newChannelName.value = "";
    conferenceNumber.value = n;
    await refreshChannelId();
    await refreshPeerCount();
    await loadPeers();
    log.value.push(`已创建频道「${name}」#${n}`);
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
    log.value.push(`已邀请好友 ${inviteToxid.value.slice(0, 8)}… 进入频道 #${conferenceNumber.value}`);
    inviteToxid.value = "";
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function copyInvite() {
  if (!ownToxid.value || !channelId.value) return;
  const text = `ToxID: ${ownToxid.value}
频道ID: ${channelId.value}
好友请求附言: join_channel ${channelId.value}`;
  try {
    await navigator.clipboard.writeText(text);
    log.value.push("邀请信息已复制");
  } catch {
    log.value.push("复制失败，请手动复制");
  }
}

async function invite() {
  if (conferenceNumber.value === null || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await api.conferenceInvite(Number(friendNumber.value), conferenceNumber.value);
    log.value.push(`已邀请好友 #${friendNumber.value} 进入频道 #${conferenceNumber.value}`);
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
    await api.conferenceSend(conferenceNumber.value, message.value.trim());
    const name = myChannels.value.find((c) => c.conferenceNumber === conferenceNumber.value)?.name || `频道 #${conferenceNumber.value}`;
    messages.value.push({ conferenceNumber: conferenceNumber.value, channelName: name, peer: "我", text: message.value.trim() });
    message.value = "";
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
  onEvent("channel:message", (e: { conferenceNumber: number; peerNumber: number; text: string }) => {
    const name = myChannels.value.find((c) => c.conferenceNumber === e.conferenceNumber)?.name || `频道 #${e.conferenceNumber}`;
    messages.value.push({ conferenceNumber: e.conferenceNumber, channelName: name, peer: `#${e.peerNumber}`, text: e.text });
  });
  onEvent("channel:connected", async (e: { conferenceNumber: number }) => {
    ensureMyChannel(e.conferenceNumber);
    conferenceNumber.value = e.conferenceNumber;
    await refreshChannelId();
    await refreshPeerCount();
    await loadPeers();
    log.value.push(`已连接频道 #${e.conferenceNumber}`);
  });
  onEvent("channel:joined", async (e: { conferenceNumber: number; friendNumber: number }) => {
    ensureMyChannel(e.conferenceNumber);
    conferenceNumber.value = e.conferenceNumber;
    await refreshChannelId();
    await refreshPeerCount();
    await loadPeers();
    log.value.push(`已接受好友 #${e.friendNumber} 的邀请，加入频道 #${e.conferenceNumber}`);
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
  <div class="panel">
    <h2>{{ t("channelsTitle") }}</h2>

    <div class="card">
      <div class="row">
        <span class="state" v-if="conferenceNumber !== null">当前频道 #{{ conferenceNumber }} · 成员 {{ peerCount }}</span>
        <span class="state" v-else>尚未创建/加入频道</span>
      </div>
      <div class="row">
        <input v-model="newChannelName" placeholder="新频道名称（可选）" @keydown.enter="create" />
        <button class="primary" :disabled="busy" @click="create">创建/加入频道</button>
      </div>
      <p v-if="error" class="error">{{ error }}</p>
    </div>

    <div class="card">
      <div class="log-title">我的频道</div>
      <div class="row">
        <input v-model="subName" placeholder="新频道名称" />
        <button :disabled="busy || !subName.trim()" @click="createSubChannel">创建频道</button>
      </div>
      <div v-if="myChannels.length === 0" class="empty">还没有频道，先创建一个吧。</div>
      <div v-for="sub in myChannels" :key="sub.conferenceNumber" class="pub-channel">
        <div class="pub-info">
          <div class="pub-name">{{ sub.name }}</div>
          <div class="pub-desc">#{{ sub.conferenceNumber }}</div>
        </div>
        <button :disabled="conferenceNumber === sub.conferenceNumber" @click="switchChannel(sub.conferenceNumber)">切换</button>
        <button class="danger" :disabled="busy" @click="deleteChannel(sub.conferenceNumber)">删除</button>
      </div>
    </div>

    <div class="card" v-if="conferenceNumber !== null">
      <label>邀请链接 / 频道 ID</label>
      <p class="tip">
        把下面的 ToxID 和频道 ID 发给朋友，让对方添加你并在好友请求附言中写：
        <code>join_channel {{ channelId }}</code>
      </p>
      <div class="mono toxid">ToxID: {{ ownToxid }}</div>
      <div class="mono toxid">频道ID: {{ channelId }}</div>
      <div class="row">
        <button @click="copyInvite">复制邀请信息</button>
        <button @click="pasteInvite">粘贴邀请并加入</button>
      </div>
    </div>

    <div class="card chat-window" v-if="conferenceNumber !== null">
      <div class="chat-header">
        <div class="chat-title">
          <span class="chat-name">{{ currentChannelName || ('#' + conferenceNumber) }}</span>
          <span class="chat-meta">#{{ conferenceNumber }} · {{ peerCount }} 人</span>
        </div>
        <div class="row">
          <button class="mini" @click="copyInvite">复制邀请</button>
          <button class="mini" @click="pasteInvite">粘贴加入</button>
        </div>
      </div>
      <div ref="chatMessagesRef" class="chat-messages">
        <div v-if="currentMessages.length === 0" class="empty chat-empty">还没有消息，来发第一条吧</div>
        <div v-for="(m, i) in currentMessages" :key="'m' + i" class="chat-msg" :class="{ mine: m.peer === '我' }">
          <div class="bubble">
            <div class="bubble-peer">{{ m.peer === '我' ? '我' : m.peer }}</div>
            <div class="bubble-text">{{ m.text }}</div>
          </div>
        </div>
      </div>
      <div class="chat-composer">
        <textarea
          v-model="message"
          rows="2"
          maxlength="1372"
          placeholder="输入消息…（Enter 发送，Shift+Enter 换行）"
          @keydown.enter.exact.prevent="send"
        ></textarea>
        <button class="primary" :disabled="busy || !message.trim()" @click="send">{{ t("send") }}</button>
      </div>
    </div>

    <div class="card" v-if="conferenceNumber !== null">
      <label>邀请好友（ToxID）</label>
      <div class="row">
        <input v-model="inviteToxid" class="mono" placeholder="好友 ToxID 或公钥" />
        <button :disabled="busy || !inviteToxid.trim()" @click="inviteByToxid">邀请</button>
      </div>
      <label>{{ t("inviteFriend") }}（好友编号，备用）</label>
      <div class="row">
        <input v-model="friendNumber" type="number" min="0" />
        <button :disabled="busy" @click="invite">{{ t("add") }}</button>
      </div>
    </div>

    <div class="card" v-if="conferenceNumber !== null">
      <div class="log-title">发布为公共频道</div>
      <input v-model="channelName" placeholder="频道名称" />
      <input v-model="channelDesc" placeholder="频道简介（可选）" />
      <button class="primary" :disabled="busy || !channelName.trim()" @click="publishChannel">发布到公共频道列表</button>
    </div>

    <div class="card" v-if="conferenceNumber !== null">
      <div class="log-title">频道成员</div>
      <p class="tip">当前频道里的用户列表，来自 Tox 会议成员信息。</p>
      <div v-if="peers.length === 0" class="empty">暂无成员信息</div>
      <div v-for="p in peers" :key="p.peerNumber" class="pub-channel">
        <div class="pub-info">
          <div class="pub-name">{{ p.name || "未知成员" }}</div>
          <div class="pub-desc mono">{{ p.publicKey.slice(0, 12) }}…</div>
        </div>
      </div>
    </div>

    <div class="card">
      <div class="log-title">公共频道</div>
      <p class="tip">发现并加入公共频道。加入后会向频道 host 发送好友请求/加入申请，host 接受后邀请你进入。</p>
      <div v-if="publicChannels.length === 0" class="empty">暂无公共频道，成为第一个 host 吧。</div>
      <div v-for="ch in publicChannels" :key="ch.channelId" class="pub-channel">
        <div class="pub-info">
          <div class="pub-name">{{ ch.name }}</div>
          <div class="pub-desc">{{ ch.desc }}</div>
        </div>
        <button :disabled="busy || joiningChannelId === ch.channelId || isChannelActive(ch) || isRequested(ch)" @click="joinPublic(ch)">
          {{ isChannelActive(ch) ? "已进入" : (isRequested(ch) ? "已申请" : (joiningChannelId === ch.channelId ? "加入中…" : "加入")) }}
        </button>
        <button @click="copyPublicInvite(ch)">复制邀请</button>
        <button v-if="ch.hosts && ch.hosts.includes(ownToxid)" class="danger" :disabled="busy" @click="deletePublic(ch)">删除</button>
        <div v-if="ch.hosts && ch.hosts.includes(ownToxid)" class="host-manage">
          <input v-model="hostInput" placeholder="添加 co-host ToxID" />
          <button :disabled="busy || !hostInput.trim()" @click="addHost(ch)">添加</button>
          <div v-for="h in ch.hosts" :key="h" class="host-row">
            <span class="mono">{{ h.slice(0, 12) }}…</span>
            <button class="mini" :disabled="busy || h === ownToxid" @click="removeHost(ch, h)">移除</button>
          </div>
        </div>
      </div>
    </div>

    <div class="card log-card">
      <div class="log-title">系统日志</div>
      <div v-if="log.length === 0" class="empty">暂无日志</div>
      <div v-for="(l, i) in log" :key="'l' + i" class="log-line">{{ l }}</div>
    </div>
  </div>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  gap: 14px;
  max-width: 640px;
}
h2 {
  font-size: 18px;
}
.card {
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.state {
  color: var(--text-dim);
  font-size: 12px;
  flex: 1;
}
.error {
  color: var(--danger);
  font-size: 12px;
}
.chat-window {
  min-height: 360px;
  max-height: 520px;
  display: flex;
  flex-direction: column;
  gap: 0;
  padding: 0;
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
.chat-messages {
  flex: 1;
  overflow-y: auto;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 240px;
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
.log-card {
  min-height: 200px;
}
.log-title {
  font-weight: 600;
  font-size: 13px;
  color: var(--text-dim);
}
.msg {
  display: flex;
  gap: 8px;
  padding: 4px 0;
  border-bottom: 1px solid var(--border);
  align-items: baseline;
}
.channel-tag {
  color: var(--text-dim);
  font-size: 11px;
  background: var(--bg-3);
  border-radius: 4px;
  padding: 1px 6px;
  white-space: nowrap;
}
.peer {
  color: var(--accent);
  font-weight: 600;
  white-space: nowrap;
}
.log-line {
  color: var(--text-dim);
  font-size: 12px;
  padding: 2px 0;
}
.card textarea {
  width: 100%;
  min-height: 44px;
  resize: vertical;
  box-sizing: border-box;
}
.pub-channel {
  display: flex;
  align-items: center;
  gap: 10px;
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 8px 10px;
}
.pub-info {
  flex: 1;
  min-width: 0;
}
.pub-name {
  font-weight: 600;
}
.pub-desc {
  color: var(--text-dim);
  font-size: 12px;
}
.tip {
  color: var(--text-dim);
  font-size: 12px;
  line-height: 1.5;
}
.host-manage {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 4px;
}
.host-row {
  display: flex;
  align-items: center;
  gap: 6px;
}
button.mini {
  padding: 2px 6px;
  font-size: 11px;
}
.toxid {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 8px;
}
code {
  background: var(--bg-3);
  border-radius: 4px;
  padding: 1px 5px;
}
</style>
