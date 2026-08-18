<script setup lang="ts">
import { onMounted, ref } from "vue";
import QRCode from "qrcode";
import { api } from "../api";
import { locale, setLocale, t } from "../i18n";
import Avatar from "./Avatar.vue";
import type { NetworkStatus, OwnInfo } from "../types";

const props = defineProps<{ own: OwnInfo | null }>();
const emit = defineEmits<{ saved: [] }>();

const name = ref("");
const bio = ref("");
const qrUrl = ref("");
const busy = ref(false);
const saved = ref("");
const imgurClientId = ref("");
const networkStatus = ref<NetworkStatus | null>(null);
const appVersion = ref("");
const mediaConfigured = ref(false);
const mediaSaved = ref("");
const autoStart = ref(false);
const autoStartBusy = ref(false);
const deviceToxid = ref("");
const deviceMsg = ref("你好，这是我的另一台设备");
const syncResult = ref("");
const syncing = ref(false);
const avatarFile = ref<HTMLInputElement | null>(null);
const avatarBusy = ref(false);
const avatarUrl = ref("");

onMounted(async () => {
  if (props.own) {
    name.value = props.own.name;
    bio.value = props.own.statusMessage;
  }
  if (props.own) {
    QRCode.toDataURL(props.own.toxid, { width: 220, margin: 1 }).then((u) => (qrUrl.value = u));
  }
  try {
    const cfg = await api.getMediaConfig();
    mediaConfigured.value = cfg.hasClientId;
  } catch {
    /* ignore */
  }
  try {
    autoStart.value = await api.getAutoStart();
  } catch {
    /* ignore */
  }
  try {
    networkStatus.value = await api.getNetworkStatus();
  } catch {
    networkStatus.value = null;
  }
  try {
    appVersion.value = await api.getAppVersion();
  } catch {
    appVersion.value = "";
  }
});

async function copyToxid() {
  if (!props.own?.toxid) return;
  try {
    await navigator.clipboard.writeText(props.own.toxid);
    saved.value = "ToxID 已复制";
  } catch {
    saved.value = "复制失败，请手动复制";
  }
}

async function saveMedia() {
  if (!imgurClientId.value.trim()) return;
  try {
    await api.setImgurClientId(imgurClientId.value.trim());
    imgurClientId.value = "";
    mediaConfigured.value = true;
    mediaSaved.value = "已保存 Imgur Client ID";
  } catch (e) {
    alert(String(e));
  }
}

async function toggleAutoStart() {
  if (autoStartBusy.value) return;
  autoStartBusy.value = true;
  const next = !autoStart.value;
  try {
    await api.setAutoStart(next);
    autoStart.value = next;
    saved.value = next ? "已开启开机自启" : "已关闭开机自启";
  } catch (e) {
    alert(String(e));
  } finally {
    autoStartBusy.value = false;
  }
}

async function addDevice() {
  if (!deviceToxid.value.trim()) return;
  try {
    await api.addFriend(deviceToxid.value.trim(), deviceMsg.value);
    deviceToxid.value = "";
    syncResult.value = "已发送设备好友请求；对方接受后会自动开始同步。";
    emit("saved");
  } catch (e) {
    syncResult.value = String(e);
  }
}

async function syncNow() {
  if (syncing.value) return;
  syncing.value = true;
  syncResult.value = "";
  try {
    const n = await api.requestSyncAll();
    syncResult.value = `已向 ${n} 个在线设备/好友发送同步请求`;
  } catch (e) {
    syncResult.value = String(e);
  } finally {
    syncing.value = false;
  }
}

async function saveAvatarUrl() {
  const url = avatarUrl.value.trim();
  if (!url) return;
  try {
    await api.setAvatarUrl(url);
    avatarUrl.value = "";
    emit("saved");
    alert("头像 URL 已保存并广播给好友");
  } catch (e) {
    alert(String(e));
  }
}

async function onAvatarSelected(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file || avatarBusy.value) return;
  if (file.size > 5 * 1024 * 1024) {
    alert("头像不能超过 5MB");
    return;
  }
  avatarBusy.value = true;
  try {
    const dataUrl = await readFileAsDataUrl(file);
    await api.setAvatar(dataUrl);
    emit("saved");
    alert("头像已更新并广播给好友");
  } catch (err) {
    alert(String(err));
  } finally {
    avatarBusy.value = false;
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
    <h2>{{ t("settingsTitle") }}</h2>

    <div class="card">
      <label>连接状态</label>
      <div class="conn-row">
        <span class="dot" :class="{ online: networkStatus?.connected }"></span>
        <span>{{ networkStatus ? (networkStatus.connected ? (networkStatus.connection === "udp" ? "UDP 已连接" : "TCP 已连接") : "未连接") : "检测中…" }}</span>
      </div>
      <div class="conn-detail">Bootstrap 节点（配置）：{{ networkStatus?.dhtNodes ?? "…" }}</div>
      <div class="conn-detail">Relay：{{ networkStatus ? (networkStatus.relayOk ? "可用" : "不可用") : "检测中…" }}</div>
      <div class="conn-detail">好友：{{ networkStatus?.friends ?? "…" }} / 在线：{{ networkStatus?.onlineFriends ?? "…" }}</div>
      <div class="conn-detail">版本：v{{ appVersion || "…" }}</div>
    </div>

    <div class="card">
      <label>{{ t("autoStart") }}</label>
      <div class="row">
        <span class="state">{{ autoStart ? t("autoStartOn") : t("autoStartOff") }}</span>
        <button class="primary" :disabled="autoStartBusy" @click="toggleAutoStart">
          {{ autoStartBusy ? t("processing") : (autoStart ? t("autoStartDisable") : t("autoStartEnable")) }}
        </button>
      </div>
    </div>

    <div class="card">
      <label>语言 / Language</label>
      <div class="row">
        <button :class="{ active: locale === 'zh' }" @click="setLocale('zh')">中文</button>
        <button :class="{ active: locale === 'en' }" @click="setLocale('en')">English</button>
      </div>
    </div>

    <div class="card">
      <label>头像</label>
      <div class="avatar-row">
        <Avatar :src="own?.avatar" :name="own?.name" :size="72" />
        <input ref="avatarFile" type="file" accept="image/*" hidden @change="onAvatarSelected" />
        <button :disabled="avatarBusy" @click="avatarFile?.click()">
          {{ avatarBusy ? "上传中…" : "上传头像" }}
        </button>
      </div>
      <div class="row">
        <input v-model="avatarUrl" placeholder="或填写图片 URL（http/https）" />
        <button :disabled="!avatarUrl.trim()" @click="saveAvatarUrl">使用 URL</button>
      </div>
      <p class="tip">头像可上传到 Imgur，也可直接使用其他图床的图片 URL。</p>
    </div>

    <div class="card">
      <label>昵称</label>
      <input v-model="name" maxlength="128" placeholder="你的昵称（广播给好友）" />
      <label>简介</label>
      <textarea v-model="bio" rows="2" maxlength="500" placeholder="一句话介绍自己"></textarea>
      <button class="primary" :disabled="busy" @click="save">保存并广播</button>
      <p v-if="saved" class="ok">{{ saved }}</p>
    </div>

    <div class="card">
      <label>多设备同步</label>
      <p class="tip">
        每台设备使用独立 Tox 身份，设备之间互加好友后，通过现有 TSP 同步协议自动补齐帖子/评论/反应。
        在另一台设备上也打开 ToxSocial，把下方 ToxID 填到对方“添加好友”，再把对方 ToxID 填到这里。
      </p>
      <div class="mono toxid">{{ own?.toxid }}</div>
      <button @click="copyToxid">复制 ToxID</button>
      <input
        v-model="deviceToxid"
        class="mono"
        placeholder="对方设备的 ToxID（76 位十六进制）"
      />
      <input v-model="deviceMsg" placeholder="设备好友请求附言" />
      <div class="row">
        <button :disabled="deviceToxid.trim().length < 70" @click="addDevice">添加设备</button>
        <button class="primary" :disabled="syncing" @click="syncNow">
          {{ syncing ? "同步中…" : "立即同步" }}
        </button>
      </div>
      <p v-if="syncResult" class="ok">{{ syncResult }}</p>
    </div>

    <div class="card">
      <label>媒体上传（Imgur）</label>
      <p class="tip">发帖时可选择图片/视频，自动上传到 Imgur 并插入 Markdown 链接，减少 Tox 网络压力。</p>
      <input
        v-model="imgurClientId"
        type="password"
        placeholder="Imgur Client ID（匿名上传用）"
      />
      <div class="row">
        <span class="state">{{ mediaConfigured ? "已配置" : "未配置" }}</span>
        <button class="primary" :disabled="!imgurClientId.trim()" @click="saveMedia">保存</button>
      </div>
      <p v-if="mediaSaved" class="ok">{{ mediaSaved }}</p>
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
.tip {
  color: var(--text-dim);
  font-size: 12px;
  line-height: 1.5;
}
.row {
  display: flex;
  align-items: center;
  gap: 10px;
}
.state {
  color: var(--text-dim);
  font-size: 12px;
}
.row button.active {
  background: var(--accent);
  color: #fff;
}
.avatar-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.conn-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.conn-detail {
  color: var(--text-dim);
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
