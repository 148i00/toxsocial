<script setup lang="ts">
import { ref } from "vue";
import { api } from "../api";
import { t } from "../i18n";
import Avatar from "./Avatar.vue";
import type { DirectoryEntryInfo, FriendInfo } from "../types";

const props = defineProps<{ friends: FriendInfo[] }>();
const emit = defineEmits<{ changed: [] }>();

const toxid = ref("");
const message = ref("你好，关注一下！");
const busy = ref(false);
const error = ref("");
const dirQuery = ref("");
const dirResults = ref<DirectoryEntryInfo[]>([]);
const dirSearching = ref(false);
const dirNote = ref("");
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

function isFriend(d: DirectoryEntryInfo): boolean {
  return props.friends.some(
    (f) => f.toxid === d.toxid || (d.pubkey.length > 0 && f.toxid.startsWith(d.pubkey)),
  );
}

async function searchDirectory() {
  const q = dirQuery.value.trim();
  if (!q || dirSearching.value) return;
  dirSearching.value = true;
  dirNote.value = "";
  try {
    const local = await api.searchDirectory(q, 50);
    let relay: DirectoryEntryInfo[] = [];
    try {
      relay = await api.searchRelayDirectory(q);
    } catch {
      // relay unavailable, ignore
    }
    const merged = [...local, ...relay];
    const seen = new Set<string>();
    const filtered = merged.filter((d) => {
      if (!d.pubkey || seen.has(d.pubkey) || isFriend(d)) return false;
      seen.add(d.pubkey);
      return true;
    });
    dirResults.value = filtered;
    const sent = await api.requestDirectorySearch(q, 2);
    dirNote.value = `已从本地/Relay 找到 ${filtered.length} 人，并向 ${sent} 位好友发起目录请求；结果会陆续同步。`;
  } catch (e) {
    dirNote.value = String(e);
  } finally {
    dirSearching.value = false;
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
    <h2>{{ t("friendsTitle") }}</h2>
    <div class="add-box">
      <input
        v-model="toxid"
        class="mono"
        :placeholder="t('addFriendPlaceholder')"
      />
      <input v-model="message" :placeholder="t('addFriendMsgPlaceholder')" />
      <button class="primary" :disabled="busy || toxid.trim().length < 70" @click="add">
        添加
      </button>
      <p v-if="error" class="error">{{ error }}</p>
      <p v-if="ok" class="ok">{{ ok }}</p>
    </div>

    <div class="add-box">
      <label>找人（本地目录 + 好友递归查找）</label>
      <input v-model="dirQuery" placeholder="输入昵称或公钥" @keydown.enter="searchDirectory" />
      <button class="primary" :disabled="dirSearching || !dirQuery.trim()" @click="searchDirectory">
        {{ dirSearching ? "搜索中…" : "搜索" }}
      </button>
      <p v-if="dirNote" class="ok">{{ dirNote }}</p>
      <div v-for="d in dirResults" :key="d.pubkey" class="friend">
        <Avatar :src="d.avatar" :name="d.name" :size="32" />
        <div class="info">
          <div class="name">{{ d.name || "未命名" }}</div>
          <div class="mono">{{ d.pubkey }}</div>
          <div v-if="d.relay" class="mono">relay: {{ d.relay }}</div>
        </div>
        <button @click="toxid = d.toxid || d.pubkey">关注</button>
      </div>
    </div>

    <div v-if="friends.length === 0" class="empty">
      {{ t("noFriends") }}
    </div>
    <div v-for="f in friends" :key="f.toxid" class="friend">
      <Avatar :src="f.avatar" :name="f.name" :size="36" />
      <span class="dot" :class="{ online: f.online }"></span>
      <div class="info">
        <div class="name">{{ f.name || "未命名好友" }}</div>
        <div class="mono">{{ f.toxid }}</div>
      </div>
      <span class="state">{{ f.online ? t("online") : t("offline") }}</span>
      <button class="danger" @click="remove(f)">{{ t("unfollow") }}</button>
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
