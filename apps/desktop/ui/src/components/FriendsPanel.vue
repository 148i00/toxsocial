<script setup lang="ts">
import { ref } from "vue";
import { api } from "../api";
import { t } from "../i18n";
import Avatar from "./Avatar.vue";
import type { FriendInfo } from "../types";

const props = defineProps<{ friends: FriendInfo[]; kind?: "friend" | "follow" }>();
const emit = defineEmits<{ changed: []; open: [pubkey: string]; pm: [pubkey: string] }>();

const removing = ref<string | null>(null);

// The two lists use different wording for the same destructive action.
const isFollow = () => props.kind === "follow";

async function remove(f: FriendInfo) {
  const label = f.name || f.toxid.slice(0, 8);
  const ask = isFollow()
    ? t("confirmUnfollow", { name: label })
    : t("confirmUnfollowFriend", { name: label });
  if (!confirm(ask)) return;
  removing.value = f.pubkey;
  try {
    // A follow is a conference subscription, not a contact: leave the
    // conference instead of deleting a friend link.
    if (isFollow()) {
      await api.unfollowUser(f.pubkey);
    } else {
      await api.removeFriendByToxid(f.toxid);
    }
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

function pm(f: FriendInfo) {
  emit("pm", f.pubkey);
}
</script>

<template>
  <div class="panel">
    <h2>{{ kind === "follow" ? t("followsTitle") : t("friendsTitle") }}</h2>

    <div v-if="friends.length === 0" class="empty">
      {{ kind === "follow" ? t("noFollows") : t("noFriends") }}
    </div>
    <div v-for="f in friends" :key="f.toxid" class="friend" @click="open(f)">
      <Avatar :src="f.avatar" :name="f.name" :size="36" />
      <span class="dot" :class="{ online: f.online }"></span>
      <div class="info">
        <div class="name">{{ f.name || t("unnamedFriend") }}</div>
        <div class="mono">{{ f.pubkey }}</div>
      </div>
      <span class="state">{{ f.online ? t("online") : t("offline") }}</span>
      <button v-if="kind !== 'follow'" :disabled="!f.online" :title="t('pmTitle')" @click.stop="pm(f)">
        {{ t("privateChat") }}
      </button>
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
