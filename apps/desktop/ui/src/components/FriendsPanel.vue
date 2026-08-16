<script setup lang="ts">
import { ref } from "vue";
import { api } from "../api";
import { t } from "../i18n";
import Avatar from "./Avatar.vue";
import type { FriendInfo } from "../types";

const props = defineProps<{ friends: FriendInfo[] }>();
const emit = defineEmits<{ changed: []; open: [pubkey: string] }>();

const removing = ref<string | null>(null);

async function remove(f: FriendInfo) {
  if (!confirm(`取消关注 ${f.name || f.toxid.slice(0, 8)}？`)) return;
  removing.value = f.pubkey;
  try {
    await api.removeFriendByToxid(f.toxid);
    emit("changed");
  } catch (e) {
    alert(String(e));
  } finally {
    removing.value = null;
  }
}

function open(f: FriendInfo) {
  emit("open", f.pubkey);
}
</script>

<template>
  <div class="panel">
    <h2>{{ t("friendsTitle") }}</h2>

    <div v-if="friends.length === 0" class="empty">
      {{ t("noFriends") }}
    </div>
    <div v-for="f in friends" :key="f.toxid" class="friend" @click="open(f)">
      <Avatar :src="f.avatar" :name="f.name" :size="36" />
      <span class="dot" :class="{ online: f.online }"></span>
      <div class="info">
        <div class="name">{{ f.name || "未命名好友" }}</div>
        <div class="mono">{{ f.pubkey }}</div>
      </div>
      <span class="state">{{ f.online ? t("online") : t("offline") }}</span>
      <button class="danger" :disabled="removing === f.pubkey" @click.stop="remove(f)">
        {{ t("unfollow") }}
      </button>
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
.friend {
  display: flex;
  align-items: center;
  gap: 10px;
  background: var(--bg-2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px 12px;
  cursor: pointer;
  transition: border-color 0.15s;
}
.friend:hover {
  border-color: var(--accent);
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
