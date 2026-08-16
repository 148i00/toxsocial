<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, onEvent } from "../api";
import { t } from "../i18n";
import type { PublicChannelInfo } from "../types";

const conferenceNumber = ref<number | null>(null);
const channelId = ref("");
const peerCount = ref(0);
const ownToxid = ref("");
const subChannels = ref<{ name: string; conferenceNumber: number }[]>([]);
const subName = ref("");
const friendNumber = ref("0");
const inviteToxid = ref("");
const message = ref("");
const messages = ref<{ peer: string; text: string }[]>([]);
const log = ref<string[]>([]);
const busy = ref(false);
const error = ref("");

const publicChannels = ref<PublicChannelInfo[]>([]);
const channelName = ref("");
const channelDesc = ref("");
const hostInput = ref("");

async function loadPublicChannels() {
  try {
    publicChannels.value = await api.listPublicChannels();
  } catch (e) {
    publicChannels.value = [];
    log.value.push(`加载公共频道失败：${e}`);
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

async function joinPublic(ch: PublicChannelInfo) {
  if (!ch.hostToxid || ch.hostToxid.length < 70) {
    log.value.push(`「${ch.name}」暂时没有可用的 host，等待频道管理员接入。`);
    return;
  }
  try {
    await api.addFriend(ch.hostToxid, `join_channel ${ch.channelId}`);
    log.value.push(`已向「${ch.name}」host 发送加入申请。`);
  } catch (e) {
    log.value.push(`加入「${ch.name}」失败：${e}`);
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

async function createSubChannel() {
  if (!subName.value.trim() || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const n = await api.conferenceNew();
    subChannels.value.push({ name: subName.value.trim(), conferenceNumber: n });
    subName.value = "";
    await switchChannel(n);
    log.value.push(`已创建子频道 #${n}`);
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
}

async function create() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    conferenceNumber.value = await api.conferenceNew();
    await refreshChannelId();
    await refreshPeerCount();
    log.value.push(`已创建频道 #${conferenceNumber.value}`);
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
    messages.value.push({ peer: "我", text: message.value.trim() });
    message.value = "";
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

onMounted(async () => {
  try {
    const info = await api.getOwnInfo();
    ownToxid.value = info.toxid;
  } catch {
    /* ignore */
  }
  await loadPublicChannels();
  onEvent("channel:message", (e: { conferenceNumber: number; peerNumber: number; text: string }) => {
    messages.value.push({ peer: `#${e.peerNumber}`, text: e.text });
  });
  onEvent("channel:connected", async (e: { conferenceNumber: number }) => {
    conferenceNumber.value = e.conferenceNumber;
    await refreshChannelId();
    await refreshPeerCount();
    log.value.push(`已连接频道 #${e.conferenceNumber}`);
  });
  onEvent("channel:joined", async (e: { conferenceNumber: number; friendNumber: number }) => {
    conferenceNumber.value = e.conferenceNumber;
    await refreshChannelId();
    await refreshPeerCount();
    log.value.push(`已接受好友 #${e.friendNumber} 的邀请，加入频道 #${e.conferenceNumber}`);
  });
  onEvent("channel:peer_list_changed", async () => {
    await refreshPeerCount();
  });
});
</script>

<template>
  <div class="panel">
    <h2>{{ t("channelsTitle") }}</h2>

    <div class="card">
      <div class="row">
        <span class="state" v-if="conferenceNumber !== null">当前频道 #{{ conferenceNumber }} · 成员 {{ peerCount }}</span>
        <span class="state" v-else>尚未创建/加入频道</span>
        <button class="primary" :disabled="busy" @click="create">创建/加入频道</button>
      </div>
      <p v-if="error" class="error">{{ error }}</p>
    </div>

    <div class="card" v-if="conferenceNumber !== null">
      <div class="log-title">子频道</div>
      <div class="row">
        <input v-model="subName" placeholder="子频道名称" />
        <button :disabled="busy || !subName.trim()" @click="createSubChannel">创建子频道</button>
      </div>
      <div v-for="sub in subChannels" :key="sub.conferenceNumber" class="pub-channel">
        <div class="pub-info">
          <div class="pub-name">{{ sub.name }}</div>
          <div class="pub-desc">#{{ sub.conferenceNumber }}</div>
        </div>
        <button :disabled="conferenceNumber === sub.conferenceNumber" @click="switchChannel(sub.conferenceNumber)">切换</button>
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
      <label>{{ t("sendMessage") }}</label>
      <div class="row">
        <input
          v-model="message"
          maxlength="1372"
          placeholder="输入频道消息"
          @keydown.enter="send"
        />
        <button class="primary" :disabled="busy || !message.trim()" @click="send">{{ t("send") }}</button>
      </div>
    </div>

    <div class="card" v-if="conferenceNumber !== null">
      <div class="log-title">发布为公共频道</div>
      <input v-model="channelName" placeholder="频道名称" />
      <input v-model="channelDesc" placeholder="频道简介（可选）" />
      <button class="primary" :disabled="busy || !channelName.trim()" @click="publishChannel">发布到公共频道列表</button>
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
        <button :disabled="busy" @click="joinPublic(ch)">加入</button>
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
      <div class="log-title">消息 / 日志</div>
      <div v-if="log.length === 0 && messages.length === 0" class="empty">还没有频道活动</div>
      <div v-for="(m, i) in messages" :key="'m' + i" class="msg">
        <span class="peer">{{ m.peer }}</span>
        <span>{{ m.text }}</span>
      </div>
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
