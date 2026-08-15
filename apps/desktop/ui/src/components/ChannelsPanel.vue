<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, onEvent } from "../api";

const conferenceNumber = ref<number | null>(null);
const friendNumber = ref("0");
const message = ref("");
const messages = ref<{ peer: string; text: string }[]>([]);
const log = ref<string[]>([]);
const busy = ref(false);
const error = ref("");

async function create() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    conferenceNumber.value = await api.conferenceNew();
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

onMounted(() => {
  onEvent("channel:message", (e: { conferenceNumber: number; peerNumber: number; text: string }) => {
    messages.value.push({ peer: `#${e.peerNumber}`, text: e.text });
  });
  onEvent("channel:connected", (e: { conferenceNumber: number }) => {
    log.value.push(`已连接频道 #${e.conferenceNumber}`);
  });
  onEvent("channel:joined", (e: { conferenceNumber: number; friendNumber: number }) => {
    log.value.push(`已接受好友 #${e.friendNumber} 的邀请，加入频道 #${e.conferenceNumber}`);
  });
});
</script>

<template>
  <div class="panel">
    <h2>频道</h2>

    <div class="card">
      <div class="row">
        <span class="state" v-if="conferenceNumber !== null">当前频道 #{{ conferenceNumber }}</span>
        <span class="state" v-else>尚未创建/加入频道</span>
        <button class="primary" :disabled="busy" @click="create">创建频道</button>
      </div>
      <p v-if="error" class="error">{{ error }}</p>
    </div>

    <div class="card" v-if="conferenceNumber !== null">
      <label>邀请好友（好友编号）</label>
      <div class="row">
        <input v-model="friendNumber" type="number" min="0" />
        <button :disabled="busy" @click="invite">邀请</button>
      </div>
      <label>发送消息</label>
      <div class="row">
        <input
          v-model="message"
          maxlength="1372"
          placeholder="输入频道消息"
          @keydown.enter="send"
        />
        <button class="primary" :disabled="busy || !message.trim()" @click="send">发送</button>
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
</style>
