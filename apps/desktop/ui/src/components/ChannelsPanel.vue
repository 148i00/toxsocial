<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { api, onEvent } from "../api";
import { channelMessages, pushChannelMessage } from "../channelStore";
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
  return myChannels.value.find((c) => c.conferenceNumber === n)?.name || `频道 #${n}`;
});
const currentMessages = computed(() =>
  channelMessages.filter((m) => m.conferenceNumber === conferenceNumber.value),
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

async function loadPublicChannels() {
  try {
    publicChannels.value = await api.listPublicChannels();
  } catch (e) {
    publicChannels.value = [];
    pushLog(`加载公共频道失败：${e}`);
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
    pushLog(`加载我的频道失败：${e}`);
  }
}

async function deletePublic(ch: PublicChannelInfo) {
  if (!confirm(`删除公共频道「${ch.name}」？`)) return;
  try {
    await api.deletePublicChannel(ch.channelId);
    await loadPublicChannels();
    pushLog(`已删除公共频道「${ch.name}」`);
  } catch (e) {
    pushLog(`删除失败：${e}`);
  }
}

async function addHost(ch: PublicChannelInfo) {
  const toxid = hostInput.value.trim();
  if (!toxid) return;
  if (toxid.length !== 64 && toxid.length !== 76) {
    pushLog("ToxID/公钥长度不正确（应为 64 位公钥或 76 位 ToxID）");
    return;
  }
  try {
    await api.addChannelHost(ch.channelId, toxid);
    hostInput.value = "";
    await loadPublicChannels();
    pushLog(`已添加 host: ${toxid.slice(0, 8)}…`);
  } catch (e) {
    pushLog(`添加 host 失败：${e}`);
  }
}

async function removeHost(ch: PublicChannelInfo, toxid: string) {
  if (!confirm(`移除 host ${toxid.slice(0, 8)}…？`)) return;
  try {
    await api.removeChannelHost(ch.channelId, toxid);
    await loadPublicChannels();
    pushLog(`已移除 host: ${toxid.slice(0, 8)}…`);
  } catch (e) {
    pushLog(`移除 host 失败：${e}`);
  }
}

async function enterOwnChannel(targetChannelId: string) {
  if (channelId.value === targetChannelId) {
    pushLog("已经在这个频道里了");
    return;
  }
  try {
    const confs = await api.listConferences();
    for (const n of confs) {
      const id = await api.getConferenceId(n);
      if (id === targetChannelId) {
        await switchChannel(n);
        pushLog("已进入自己创建的频道");
        return;
      }
    }
    pushLog("没有找到自己的频道，请先在频道页创建/加入");
  } catch (e) {
    pushLog(`进入自己频道失败：${e}`);
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
      pushLog(`已向「${ch.name}」成员 ${contact.slice(0, 8)}… 发送加入申请。`);
      return true;
    }
    await api.addFriend(contact, `join_channel ${ch.channelId}`);
    if (!requestedChannels.value.includes(ch.channelId)) requestedChannels.value.push(ch.channelId);
    pushLog(`已向「${ch.name}」成员 ${contact.slice(0, 8)}… 发送好友请求/加入申请。`);
    return true;
  } catch (e) {
    const msg = String(e);
    if (msg.includes("已发送") || msg.includes("already")) {
      if (!requestedChannels.value.includes(ch.channelId)) requestedChannels.value.push(ch.channelId);
      pushLog(`加入「${ch.name}」：已向 ${contact.slice(0, 8)}… 发送过申请，等待接受。`);
      return true;
    }
    return false;
  }
}

async function joinPublic(ch: PublicChannelInfo) {
  if (joiningChannelId.value) return;
  joiningChannelId.value = ch.channelId;
  try {
    if (isOwnChannel(ch)) {
      await enterOwnChannel(ch.channelId);
      return;
    }
    const contacts = channelContacts(ch);
    if (contacts.length === 0) {
      pushLog(`「${ch.name}」暂时没有可用的成员，等待频道管理员接入。`);
      return;
    }
    for (const contact of contacts) {
      const ok = await tryJoinViaContact(ch, contact);
      if (ok) return;
    }
    pushLog(`加入「${ch.name}」失败：尝试了所有已知成员，均未成功。`);
  } catch (e) {
    pushLog(`加入「${ch.name}」发生错误：${e}`);
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
    pushLog(`「${ch.name}」暂时没有可用的邀请信息。`);
    return;
  }
  const text = `ToxID: ${host}
频道ID: ${ch.channelId}
好友请求附言: join_channel ${ch.channelId}`;
  try {
    await navigator.clipboard.writeText(text);
    pushLog(`已复制「${ch.name}」邀请信息`);
  } catch {
    pushLog("复制失败，请手动复制");
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
    pushLog(`已把频道「${channelName.value.trim()}」发布为公共频道`);
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
      pushLog("剪贴板里没有找到有效的频道邀请信息");
      return;
    }
    await api.addFriend(toxid, `join_channel ${ch}`);
    pushLog("已粘贴邀请并发送加入申请");
  } catch (e) {
    pushLog(`粘贴失败：${e}`);
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
    pushLog(`已删除频道 #${n}`);
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
    pushLog(`已创建频道「${name}」#${n}`);
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
    pushLog(`已创建频道「${name}」#${n}`);
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
    pushLog(`已邀请好友 ${inviteToxid.value.slice(0, 8)}… 进入频道 #${conferenceNumber.value}`);
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
    pushLog("邀请信息已复制");
  } catch {
    pushLog("复制失败，请手动复制");
  }
}

async function invite() {
  if (conferenceNumber.value === null || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await api.conferenceInvite(Number(friendNumber.value), conferenceNumber.value);
    pushLog(`已邀请好友 #${friendNumber.value} 进入频道 #${conferenceNumber.value}`);
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
    pushChannelMessage({ conferenceNumber: conferenceNumber.value, channelName: name, peer: "我", text: message.value.trim() });
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
  onEvent("channel:connected", async (e: { conferenceNumber: number }) => {
    ensureMyChannel(e.conferenceNumber);
    conferenceNumber.value = e.conferenceNumber;
    await refreshChannelId();
    await refreshPeerCount();
    await loadPeers();
    pushLog(`已连接频道 #${e.conferenceNumber}`);
  });
  onEvent("channel:joined", async (e: { conferenceNumber: number; friendNumber: number }) => {
    ensureMyChannel(e.conferenceNumber);
    conferenceNumber.value = e.conferenceNumber;
    await refreshChannelId();
    await refreshPeerCount();
    await loadPeers();
    pushLog(`已接受好友 #${e.friendNumber} 的邀请，加入频道 #${e.conferenceNumber}`);
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
        <span class="sidebar-title">频道</span>
        <button class="primary small" :disabled="busy" @click="create">＋ 创建/加入</button>
      </div>
      <div class="sidebar-create-row">
        <input v-model="newChannelName" placeholder="新频道名称（可选）" @keydown.enter="create" />
      </div>

      <div class="channel-list">
        <div v-if="myChannels.length === 0" class="empty">暂无频道，先创建一个吧</div>
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
          <button class="mini danger" :disabled="busy" @click.stop="deleteChannel(sub.conferenceNumber)">删</button>
        </div>
      </div>

      <details class="public-section">
        <summary>公共频道</summary>
        <div v-if="publicChannels.length === 0" class="empty">暂无公共频道</div>
        <div v-for="ch in publicChannels" :key="ch.channelId" class="public-item">
          <div class="public-item-info" @click="joinPublic(ch)">
            <div class="channel-item-name">{{ ch.name }}</div>
            <div class="channel-item-desc">{{ ch.desc || "暂无简介" }}</div>
          </div>
          <div class="public-item-actions">
            <button class="mini" :disabled="busy || joiningChannelId === ch.channelId || isChannelActive(ch) || isRequested(ch)" @click="joinPublic(ch)">
              {{ isChannelActive(ch) ? "已进入" : (isRequested(ch) ? "已申请" : "加入") }}
            </button>
            <button class="mini" @click="copyPublicInvite(ch)">复制</button>
            <button v-if="ch.hosts && ch.hosts.includes(ownToxid)" class="mini danger" @click="deletePublic(ch)">删</button>
          </div>
          <details v-if="ch.hosts && ch.hosts.includes(ownToxid)" class="host-manage">
            <summary>管理 host</summary>
            <div class="row">
              <input v-model="hostInput" placeholder="添加 co-host ToxID" />
              <button class="mini" :disabled="busy || !hostInput.trim()" @click="addHost(ch)">添加</button>
            </div>
            <div v-for="h in ch.hosts" :key="h" class="host-row">
              <span class="mono">{{ h.slice(0, 12) }}…</span>
              <button class="mini danger" :disabled="busy || h === ownToxid" @click="removeHost(ch, h)">移除</button>
            </div>
          </details>
        </div>
      </details>

      <div class="log-section">
        <div class="sidebar-title">系统日志</div>
        <div v-if="log.length === 0" class="empty">暂无日志</div>
        <div v-for="(l, i) in log" :key="'l' + i" class="log-line">{{ l }}</div>
      </div>
    </div>

    <!-- Right: chat window -->
    <div class="chat-main">
      <template v-if="conferenceNumber !== null">
        <div class="chat-header">
          <div class="chat-title">
            <span class="chat-name">{{ currentChannelName || ('#' + conferenceNumber) }}</span>
            <span class="chat-meta">#{{ conferenceNumber }} · {{ peerCount }} 人</span>
          </div>
          <div class="chat-actions">
            <button class="mini" @click="copyInvite">复制邀请</button>
            <button class="mini" @click="pasteInvite">粘贴加入</button>
            <button class="mini" @click="showManage = !showManage">{{ showManage ? "收起管理" : "管理" }}</button>
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
          <button class="primary" :disabled="busy || !message.trim()" @click="send">发送</button>
        </div>

        <div v-if="showManage" class="manage-panel">
          <div class="manage-grid">
            <div class="card">
              <div class="log-title">邀请链接 / 频道 ID</div>
              <div class="mono toxid">ToxID: {{ ownToxid }}</div>
              <div class="mono toxid">频道ID: {{ channelId }}</div>
              <div class="row">
                <button class="mini" @click="copyInvite">复制邀请</button>
                <button class="mini" @click="pasteInvite">粘贴加入</button>
              </div>
            </div>
            <div class="card">
              <div class="log-title">邀请好友</div>
              <div class="row">
                <input v-model="inviteToxid" class="mono" placeholder="好友 ToxID 或公钥" />
                <button :disabled="busy || !inviteToxid.trim()" @click="inviteByToxid">邀请</button>
              </div>
              <div class="row">
                <input v-model="friendNumber" type="number" min="0" />
                <button :disabled="busy" @click="invite">邀请编号好友</button>
              </div>
            </div>
            <div class="card">
              <div class="log-title">发布为公共频道</div>
              <input v-model="channelName" placeholder="频道名称" />
              <input v-model="channelDesc" placeholder="频道简介（可选）" />
              <button class="primary" :disabled="busy || !channelName.trim()" @click="publishChannel">发布</button>
            </div>
            <div class="card">
              <div class="log-title">频道成员</div>
              <div v-if="peers.length === 0" class="empty">暂无成员</div>
              <div v-for="p in peers" :key="p.peerNumber" class="member-row">
                <span>{{ p.name || "未知成员" }}</span>
                <span class="mono">{{ p.publicKey.slice(0, 10) }}…</span>
              </div>
            </div>
          </div>
        </div>
      </template>
      <div v-else class="chat-placeholder">
        <div class="empty">请选择或创建一个频道开始群聊</div>
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

