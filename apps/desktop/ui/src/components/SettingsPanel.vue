<script setup lang="ts">
import { onMounted, ref } from "vue";
import QRCode from "qrcode";
import { api } from "../api";
import type { OwnInfo } from "../types";

const props = defineProps<{ own: OwnInfo | null }>();
const emit = defineEmits<{ saved: [] }>();

const name = ref("");
const bio = ref("");
const qrUrl = ref("");
const busy = ref(false);
const saved = ref("");

onMounted(() => {
  if (props.own) {
    name.value = props.own.name;
    bio.value = props.own.statusMessage;
  }
  if (props.own) {
    QRCode.toDataURL(props.own.toxid, { width: 220, margin: 1 }).then((u) => (qrUrl.value = u));
  }
});

async function save() {
  if (busy.value) return;
  busy.value = true;
  saved.value = "";
  try {
    await api.setProfile(name.value, bio.value);
    saved.value = "已保存，并广播给所有在线好友";
    emit("saved");
  } catch (e) {
    alert(String(e));
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="panel">
    <h2>设置</h2>

    <div class="card">
      <label>昵称</label>
      <input v-model="name" maxlength="128" placeholder="你的昵称（广播给好友）" />
      <label>简介</label>
      <textarea v-model="bio" rows="2" maxlength="500" placeholder="一句话介绍自己"></textarea>
      <button class="primary" :disabled="busy" @click="save">保存并广播</button>
      <p v-if="saved" class="ok">{{ saved }}</p>
    </div>

    <div class="card">
      <label>我的身份（ToxID）</label>
      <div class="mono toxid">{{ own?.toxid }}</div>
      <div class="qr-row">
        <img v-if="qrUrl" :src="qrUrl" alt="ToxID QR" />
        <div class="qr-tip">
          扫码或复制 ToxID 分享给朋友，对方添加后即互相关注。
          <br /><br />
          <span class="tag">公钥</span>
          <div class="mono">{{ own?.pubkey }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  gap: 14px;
  max-width: 560px;
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
label {
  font-size: 12px;
  color: var(--text-dim);
}
.card button {
  align-self: flex-end;
  margin-top: 6px;
}
.ok {
  color: var(--accent-2);
  font-size: 12px;
}
.toxid {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 8px;
}
.qr-row {
  display: flex;
  gap: 16px;
  align-items: flex-start;
  margin-top: 8px;
}
.qr-row img {
  border-radius: 6px;
  background: #fff;
  padding: 6px;
  width: 220px;
  height: 220px;
}
.qr-tip {
  color: var(--text-dim);
  font-size: 13px;
  line-height: 1.6;
  flex: 1;
}
</style>
