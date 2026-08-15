<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, onEvent } from "../api";
import { t } from "../i18n";

const conferenceNumber = ref<number | null>(null);
const channelId = ref("");
const ownToxid = ref("");
const friendNumber = ref("0");
const message = ref("");
const messages = ref<{ peer: string; text: string }[]>([]);
const log = ref<string[]>([]);
const busy = ref(false);
const error = ref("");

interface PublicChannel {
  name: string;
  desc: string;
  hostToxid: string;
}

// 公共频道目录：实际加入需要 host 在线并实现自动邀请。
// 后续可改为从远程目录服务拉取。
const publicChannels: PublicChannel[] = [
  {
    name: "ToxSocial 官方频道",
    desc: "项目讨论与公告",
    hostToxid: "",
  },
  {
    name: "去中心化闲聊",
    desc: "聊技术、聊生活、聊自由软件",
    hostToxid: "",
  },
  {
    name: "Tox 中文社区",
    desc: "Tox 协议中文用户交流",
    hostToxid: "",
  },
];

async function joinPublic(ch: PublicChannel) {
  if (!ch.hostToxid || ch.hostToxid.length < 70) {
    log.value.push(`「${ch.name}」暂时没有可用的 host，等待频道管理员接入。`);
    return;
  }
  try {
    await api.addFriend(ch.hostToxid, `我想加入公共频道：${ch.name}`);
    log.value.push(`已向「${ch.name}」host 发送好友请求/加入申请。`);
  } catch (e) {
    log.value.push(`加入「${ch.name}」失败：${e}`);
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

async function create() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    conferenceNumber.value = await api.conferenceNew();
    await refreshChannelId();
    log.value.push(`已创建频道 #${conferenceNumber.value}`);
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
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
  onEvent("channel:message", (e: { conferenceNumber: number; peerNumber: number; text: string }) => {
    messages.value.push({ peer: `#${e.peerNumber}`, text: e.text });
  });
  onEvent("channel:connected", async (e: { conferenceNumber: number }) => {
    conferenceNumber.value = e.conferenceNumber;
    await refreshChannelId();
    log.value.push(`已连接频道 #${e.conferenceNumber}`);
  });
  onEvent("channel:joined", async (e: { conferenceNumber: number; friendNumber: number }) => {
    conferenceNumber.value = e.conferenceNumber;
    await refreshChannelId();
    log.value.push(`已接受好友 #${e.friendNumber} 的邀请，加入频道 #${e.conferenceNumber}`);
  });
});
</script>

<template>
  <div class="panel">
    <h2>{{ t("channelsTitle") }}</h2>

    <div class="card">
      <div class="row">
        <span class="state" v-if="conferenceNumber !== null">当前频道 #{{ conferenceNumber }}</span>
        <span class="state" v-else>尚未创建/加入频道</span>
        <button class="primary" :disabled="busy" @click="create">{{ t("createChannel") }}</button>
      </div>
      <p v-if="error" class="error">{{ error }}</p>
    </div>

    <div class="card" v-if="conferenceNumber !== null">
      <label>邀请链接 / 频道 ID</label>
      <p class="tip">
        把下面的 ToxID 和频道 ID 发给朋友，让对方添加你并在好友请求附言中写：
        <code>join_channel {{ channelId }}</code>
      </p>
      <div class="mono toxid">ToxID: {{ ownToxid }}</div>
      <div class="mono toxid">频道ID: {{ channelId }}</div>
    </div>

    <div class="card" v-if="conferenceNumber !== null">
      <label>{{ t("inviteFriend") }}</label>
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

    <div class="card">
      <div class="log-title">公共频道</div>
      <p class="tip">发现并加入公共频道。加入后会向频道 host 发送好友请求/加入申请，host 接受后邀请你进入。</p>
      <div v-for="ch in publicChannels" :key="ch.name" class="pub-channel">
        <div class="pub-info">
          <div class="pub-name">{{ ch.name }}</div>
          <div class="pub-desc">{{ ch.desc }}</div>
        </div>
        <button :disabled="busy" @click="joinPublic(ch)">加入</button>
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
