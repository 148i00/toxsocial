<script setup lang="ts">
import { ref } from "vue";
import { api } from "../api";
import type { FriendInfo } from "../types";

defineProps<{ friends: FriendInfo[] }>();
const emit = defineEmits<{ changed: [] }>();

const toxid = ref("");
const message = ref("你好，关注一下！");
const busy = ref(false);
const error = ref("");
const ok = ref("");

async function add() {
  const t = toxid.value.trim();
  if (!t || busy.value) return;
  busy.value = true;
  error.value = "";
  ok.value = "";
  try {
    const n = await api.addFriend(t, message.value);
    ok.value = `已发送好友请求（#${n}），对方接受后自动互相关注。`;
    toxid.value = "";
    emit("changed");
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function remove(f: FriendInfo) {
  if (!confirm(`取消关注 ${f.name || f.toxid.slice(0, 8)}？`)) return;
  try {
    await api.removeFriendByToxid(f.toxid);
    emit("changed");
  } catch (e) {
    alert(String(e));
  }
}
</script>

<template>
  <div class="panel">
    <h2>关注管理</h2>
    <div class="add-box">
      <input
        v-model="toxid"
        class="mono"
        placeholder="粘贴好友的 ToxID（76 位十六进制）"
      />
      <input v-model="message" placeholder="好友请求附言" />
      <button class="primary" :disabled="busy || toxid.trim().length < 70" @click="add">
        添加
      </button>
      <p v-if="error" class="error">{{ error }}</p>
      <p v-if="ok" class="ok">{{ ok }}</p>
    </div>

    <div v-if="friends.length === 0" class="empty">
      还没有好友。把上方输入框换成你的 ToxID 发给别人，或粘贴对方的 ToxID 添加。
    </div>
    <div v-for="f in friends" :key="f.toxid" class="friend">
      <span class="dot" :class="{ online: f.online }"></span>
      <div class="info">
        <div class="name">{{ f.name || "未命名好友" }}</div>
        <div class="mono">{{ f.toxid }}</div>
      </div>
      <span class="state">{{ f.online ? "在线" : "离线" }}</span>
      <button class="danger" @click="remove(f)">取关</button>
    </div>
  </div>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  gap: 14px;
}
h2 {
  font-size: 18px;
}
.add-box {
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: stretch;
}
.add-box button {
  align-self: flex-end;
}
.error {
  color: var(--danger);
  font-size: 12px;
}
.ok {
  color: var(--accent-2);
  font-size: 12px;
}
.friend {
  display: flex;
  align-items: center;
  gap: 10px;
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px 12px;
}
.info {
  flex: 1;
  min-width: 0;
}
.name {
  font-weight: 600;
}
.state {
  color: var(--text-dim);
  font-size: 12px;
}
</style>
