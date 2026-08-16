<script setup lang="ts">
import { ref } from "vue";
import { api } from "../api";
import { t } from "../i18n";
import Avatar from "./Avatar.vue";
import type { FriendInfo } from "../types";

const props = defineProps<{ friends: FriendInfo[] }>();
const emit = defineEmits<{ changed: []; open: [pubkey: string] }>();

const removing = ref<string | null>(null);
const sendingFile = ref<string | null>(null);
const fileTarget = ref<FriendInfo | null>(null);
const fileInput = ref<HTMLInputElement | null>(null);

function chooseFile(f: FriendInfo) {
  fileTarget.value = f;
  fileInput.value?.click();
}

async function onFileSelected(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  const target = fileTarget.value;
  fileTarget.value = null;
  if (!file || !target || sendingFile.value) return;
  if (file.size > 20 * 1024 * 1024) {
    alert("文件不能超过 20MB");
    return;
  }
  sendingFile.value = target.pubkey;
  try {
    const dataUrl = await readFileAsDataUrl(file);
    await api.sendFileToFriendByToxid(target.toxid, file.name, dataUrl);
    alert(`已向 ${target.name || "好友"} 发送文件：${file.name}`);
  } catch (err) {
    alert(String(err));
  } finally {
    sendingFile.value = null;
  }
}

function readFileAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result || ""));
    reader.onerror = () => reject(reader.error || new Error("read failed"));
    reader.readAsDataURL(file);
  });
}

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

    <input ref="fileInput" type="file" hidden @change="onFileSelected" />

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
      <button :disabled="sendingFile === f.pubkey" @click.stop="chooseFile(f)">
        {{ sendingFile === f.pubkey ? "发送中…" : "文件" }}
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
