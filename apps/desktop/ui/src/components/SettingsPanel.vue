<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import QRCode from "qrcode";
import { api } from "../api";
import { locale, setLocale, t } from "../i18n";
import Avatar from "./Avatar.vue";
import type { NetworkStatus, OwnInfo } from "../types";

const props = defineProps<{ own: OwnInfo | null }>();
const emit = defineEmits<{ saved: [] }>();

type Tab = "account" | "network" | "notifications" | "data" | "about";
const tab = ref<Tab>("account");
const tabs = computed(() => [
  { id: "account" as Tab, label: t("tabAccount") },
  { id: "network" as Tab, label: t("tabNetwork") },
  { id: "notifications" as Tab, label: t("tabNotifications") },
  { id: "data" as Tab, label: t("tabData") },
  { id: "about" as Tab, label: t("tabAbout") },
]);

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
const relayUrl = ref("");
const relaySaved = ref("");
const relayBusy = ref(false);
const autoStart = ref(false);
const autoStartBusy = ref(false);
const deviceToxid = ref("");
const deviceMsg = ref(t("deviceMessageDefault"));
const syncResult = ref("");
const avatarFile = ref<HTMLInputElement | null>(null);
const avatarBusy = ref(false);
const avatarUrl = ref("");
const updateStatus = ref<"idle" | "checking" | "update" | "ok" | "error">("idle");
const updateLatest = ref("");

// Notifications preferences (persisted in localStorage).
const notifyPosts = ref(true);
const notifyGroups = ref(true);
const notifyPm = ref(true);
const notifyFiles = ref(true);

// Data management.
const dbStats = ref<{ dbSizeBytes: number; postCount: number; channelMsgCount: number; privateMsgCount: number } | null>(null);
const cleanupBusy = ref(false);
const cleanupResult = ref("");

function loadNotifyPrefs() {
  try {
    notifyPosts.value = localStorage.getItem("notify_posts") !== "0";
    notifyGroups.value = localStorage.getItem("notify_groups") !== "0";
    notifyPm.value = localStorage.getItem("notify_pm") !== "0";
    notifyFiles.value = localStorage.getItem("notify_files") !== "0";
  } catch {
    /* ignore */
  }
}

function saveNotifyPref(key: string, value: boolean) {
  try {
    localStorage.setItem(key, value ? "1" : "0");
  } catch {
    /* ignore */
  }
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

async function refreshDbStats() {
  try {
    dbStats.value = await api.dbStats();
  } catch {
    dbStats.value = null;
  }
}

async function runCleanup() {
  if (cleanupBusy.value) return;
  if (!confirm(t("cleanupConfirm"))) return;
  cleanupBusy.value = true;
  try {
    const r = await api.cleanupDatabase();
    cleanupResult.value = t("cleanupDone", {
      posts: r.removedPosts,
      groups: r.removedChannelMsgs,
      pm: r.removedPrivateMsgs,
      size: formatBytes(r.dbSizeBytes),
    });
    await refreshDbStats();
  } catch (e) {
    alert(String(e));
  } finally {
    cleanupBusy.value = false;
  }
}

async function exportAccount() {
  try {
    const b64 = await api.exportAccount();
    const blob = new Blob([b64], { type: "text/plain" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `toxsocial-account-${new Date().toISOString().slice(0, 10)}.bak.txt`;
    a.click();
    URL.revokeObjectURL(a.href);
  } catch (e) {
    alert(String(e));
  }
}

async function checkForUpdate() {
  updateStatus.value = "checking";
  try {
    const info = await api.checkUpdate();
    if (info.hasUpdate) {
      updateLatest.value = info.latest;
      updateStatus.value = "update";
    } else {
      updateStatus.value = "ok";
    }
  } catch {
    updateStatus.value = "error";
  }
}

let statusTimer: ReturnType<typeof setInterval> | undefined;

async function refreshNetworkStatus() {
  try {
    networkStatus.value = await api.getNetworkStatus();
  } catch {
    networkStatus.value = null;
  }
}

onMounted(async () => {
  loadNotifyPrefs();
  if (props.own) {
    name.value = props.own.name;
    bio.value = props.own.statusMessage;
    QRCode.toDataURL(props.own.toxid, { width: 220, margin: 1 }).then((u) => (qrUrl.value = u));
  }
  try {
    const cfg = await api.getMediaConfig();
    mediaConfigured.value = cfg.hasClientId;
  } catch {
    /* ignore */
  }
  try {
    relayUrl.value = (await api.getRelayUrls()).join("\n");
  } catch {
    /* ignore */
  }
  try {
    autoStart.value = await api.getAutoStart();
  } catch {
    /* ignore */
  }
  await refreshNetworkStatus();
  statusTimer = setInterval(refreshNetworkStatus, 5_000);
  try {
    appVersion.value = await api.getAppVersion();
  } catch {
    appVersion.value = "";
  }
  await refreshDbStats();
});

onUnmounted(() => {
  if (statusTimer) clearInterval(statusTimer);
});

async function copyToxid() {
  if (!props.own?.toxid) return;
  try {
    await navigator.clipboard.writeText(props.own.toxid);
    saved.value = t("toxidCopied");
  } catch {
    saved.value = t("copyFailed");
  }
}

async function saveMedia() {
  if (!imgurClientId.value.trim()) return;
  try {
    await api.setImgurClientId(imgurClientId.value.trim());
    imgurClientId.value = "";
    mediaConfigured.value = true;
    mediaSaved.value = t("imgurSaved");
  } catch (e) {
    alert(String(e));
  }
}

async function saveRelay() {
  if (relayBusy.value || !relayUrl.value.trim()) return;
  relayBusy.value = true;
  relaySaved.value = "";
  const urls = relayUrl.value
    .split("\n")
    .map((u) => u.trim())
    .filter(Boolean);
  if (urls.length === 0) {
    relayBusy.value = false;
    alert(t("relayRequired"));
    return;
  }
  try {
    await api.setRelayUrls(urls);
    relaySaved.value = t("relaySavedCount", { count: urls.length });
  } catch (e) {
    alert(String(e));
  } finally {
    relayBusy.value = false;
  }
}

async function toggleAutoStart() {
  if (autoStartBusy.value) return;
  autoStartBusy.value = true;
  const next = !autoStart.value;
  try {
    await api.setAutoStart(next);
    autoStart.value = next;
    saved.value = next ? t("autoStartEnabled") : t("autoStartDisabled");
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
    syncResult.value = t("deviceRequestSent");
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
    syncResult.value = t("syncRequestSent", { count: n });
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
    alert(t("avatarUrlSaved"));
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
    alert(t("fileTooLarge", { size: "5MB" }));
    return;
  }
  avatarBusy.value = true;
  try {
    const dataUrl = await readFileAsDataUrl(file);
    await api.setAvatar(dataUrl);
    emit("saved");
    alert(t("avatarUpdated"));
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
    reader.onerror = () => reject(reader.error || new Error(t("readFailed")));
    reader.readAsDataURL(file);
  });
}

async function save() {
  if (busy.value) return;
  busy.value = true;
  saved.value = "";
  try {
    await api.setProfile(name.value, bio.value);
    saved.value = t("profileSaved");
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

    <!-- Tab bar -->
    <div class="tab-bar">
      <button
        v-for="tb in tabs"
        :key="tb.id"
        :class="{ active: tab === tb.id }"
        @click="tab = tb.id"
      >
        {{ tb.label }}
      </button>
    </div>

    <!-- Account tab: profile / avatar / identity / device sync -->
    <template v-if="tab === 'account'">
      <div class="card">
        <label>{{ t("nickname") }}</label>
        <input v-model="name" maxlength="128" :placeholder="t('nicknamePlaceholder')" />
        <label>{{ t("bio") }}</label>
        <textarea v-model="bio" rows="2" maxlength="500" :placeholder="t('bioPlaceholder')"></textarea>
        <button class="primary" :disabled="busy" @click="save">{{ t("saveAndBroadcast") }}</button>
        <p v-if="saved" class="ok">{{ saved }}</p>
      </div>

      <div class="card">
        <label>{{ t("avatar") }}</label>
        <div class="avatar-row">
          <Avatar :src="own?.avatar" :name="own?.name" :size="72" />
          <input ref="avatarFile" type="file" accept="image/*" hidden @change="onAvatarSelected" />
          <button :disabled="avatarBusy" @click="avatarFile?.click()">
            {{ avatarBusy ? t("uploading") : t("uploadAvatar") }}
          </button>
        </div>
        <div class="row">
          <input v-model="avatarUrl" :placeholder="t('avatarUrlPlaceholder')" />
          <button :disabled="!avatarUrl.trim()" @click="saveAvatarUrl">{{ t("useUrl") }}</button>
        </div>
        <p class="tip">{{ t("avatarTip") }}</p>
      </div>

      <div class="card">
        <label>{{ t("myIdentity") }}</label>
        <div class="mono toxid">{{ own?.toxid }}</div>
        <div class="qr-row">
          <img v-if="qrUrl" :src="qrUrl" :alt="t('toxidQr')" />
          <div class="qr-tip">
            {{ t("identityTip") }}
            <br /><br />
            <span class="tag">{{ t("publicKey") }}</span>
            <div class="mono">{{ own?.pubkey }}</div>
          </div>
        </div>
      </div>

      <div class="card">
        <label>{{ t("deviceSync") }}</label>
        <p class="tip">{{ t("deviceSyncTip") }}</p>
        <div class="mono toxid">{{ own?.toxid }}</div>
        <button @click="copyToxid">{{ t("copyToxid") }}</button>
        <input v-model="deviceToxid" class="mono" :placeholder="t('deviceToxidPlaceholder')" />
        <input v-model="deviceMsg" :placeholder="t('deviceMsgPlaceholder')" />
        <div class="row">
          <button :disabled="deviceToxid.trim().length < 70" @click="addDevice">{{ t("addDevice") }}</button>
          <button class="primary" :disabled="syncing" @click="syncNow">
            {{ syncing ? t("syncing") : t("syncNow") }}
          </button>
        </div>
        <p v-if="syncResult" class="ok">{{ syncResult }}</p>
      </div>

      <div class="card">
        <label>{{ t("mediaUpload") }}</label>
        <p class="tip">{{ t("mediaTip") }}</p>
        <input v-model="imgurClientId" type="password" :placeholder="t('imgurClientIdPlaceholder')" />
        <div class="row">
          <span class="state">{{ mediaConfigured ? t("configured") : t("notConfigured") }}</span>
          <button class="primary" :disabled="!imgurClientId.trim()" @click="saveMedia">{{ t("save") }}</button>
        </div>
        <p v-if="mediaSaved" class="ok">{{ mediaSaved }}</p>
      </div>
    </template>

    <!-- Network tab: status / relay / autostart -->
    <template v-else-if="tab === 'network'">
      <div class="card">
        <label>{{ t("connectionStatus") }}</label>
        <div class="conn-row">
          <span class="dot" :class="{ online: networkStatus?.connected }"></span>
          <span>{{ networkStatus ? (networkStatus.connected ? (networkStatus.connection === "udp" ? t("udpConnected") : t("tcpConnected")) : t("disconnected")) : t("checking") }}</span>
        </div>
        <div class="conn-detail">{{ t("bootstrapNodes", { nodes: networkStatus?.dhtNodes ?? "…" }) }}</div>
        <div class="conn-detail">{{ t("relayStatus", { status: networkStatus ? (networkStatus.relayOk ? t("relayOk") : t("relayDown")) : t("checking") }) }}</div>
        <div class="conn-detail">{{ t("friendStats", { friends: networkStatus?.friends ?? "…", online: networkStatus?.onlineFriends ?? "…" }) }}</div>
      </div>

      <div class="card">
        <label>{{ t("relayServers") }}</label>
        <textarea v-model="relayUrl" class="mono" rows="3" placeholder="https://relay1.example.com&#10;https://relay2.example.com"></textarea>
        <div class="row">
          <button class="primary" :disabled="relayBusy || !relayUrl.trim()" @click="saveRelay">
            {{ relayBusy ? t("processing") : t("save") }}
          </button>
        </div>
        <p v-if="relaySaved" class="ok">{{ relaySaved }}</p>
        <p class="tip">{{ t("relayTip") }}</p>
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
    </template>

    <!-- Notifications tab -->
    <template v-else-if="tab === 'notifications'">
      <div class="card">
        <label>{{ t("notificationsPref") }}</label>
        <label class="toggle-row">
          <input v-model="notifyPosts" type="checkbox" @change="saveNotifyPref('notify_posts', notifyPosts)" />
          {{ t("notifyPosts") }}
        </label>
        <label class="toggle-row">
          <input v-model="notifyGroups" type="checkbox" @change="saveNotifyPref('notify_groups', notifyGroups)" />
          {{ t("notifyGroups") }}
        </label>
        <label class="toggle-row">
          <input v-model="notifyPm" type="checkbox" @change="saveNotifyPref('notify_pm', notifyPm)" />
          {{ t("notifyPm") }}
        </label>
        <label class="toggle-row">
          <input v-model="notifyFiles" type="checkbox" @change="saveNotifyPref('notify_files', notifyFiles)" />
          {{ t("notifyFiles") }}
        </label>
        <p class="tip">{{ t("notifyTip") }}</p>
      </div>
    </template>

    <!-- Data tab: stats / cleanup / export -->
    <template v-else-if="tab === 'data'">
      <div class="card">
        <label>{{ t("storageUsage") }}</label>
        <div class="conn-detail">{{ t("dbSize", { size: formatBytes(dbStats?.dbSizeBytes ?? 0) }) }}</div>
        <div class="conn-detail">{{ t("statPosts", { count: dbStats?.postCount ?? 0 }) }}</div>
        <div class="conn-detail">{{ t("statGroupMsgs", { count: dbStats?.channelMsgCount ?? 0 }) }}</div>
        <div class="conn-detail">{{ t("statPmMsgs", { count: dbStats?.privateMsgCount ?? 0 }) }}</div>
      </div>
      <div class="card">
        <label>{{ t("cleanupTitle") }}</label>
        <p class="tip">{{ t("cleanupTip") }}</p>
        <div class="row">
          <button class="primary" :disabled="cleanupBusy" @click="runCleanup">
            {{ cleanupBusy ? t("processing") : t("cleanupRun") }}
          </button>
        </div>
        <p v-if="cleanupResult" class="ok">{{ cleanupResult }}</p>
      </div>
      <div class="card">
        <label>{{ t("exportTitle") }}</label>
        <p class="tip">{{ t("exportTip") }}</p>
        <div class="row">
          <button class="primary" @click="exportAccount">{{ t("exportRun") }}</button>
        </div>
      </div>
    </template>

    <!-- About tab: version / update / language -->
    <template v-else>
      <div class="card">
        <label>{{ t("aboutTitle") }}</label>
        <div class="conn-detail">{{ t("version", { version: appVersion || "…" }) }}</div>
        <div class="conn-detail">
          <span v-if="updateStatus === 'checking'">{{ t("checkingUpdate") }}</span>
          <span v-else-if="updateStatus === 'update'">{{ t("updateAvailable", { version: updateLatest }) }}</span>
          <span v-else-if="updateStatus === 'ok'">{{ t("upToDate") }}</span>
          <span v-else-if="updateStatus === 'error'">{{ t("updateCheckFailed") }}</span>
          <button class="mini" :disabled="updateStatus === 'checking'" @click="checkForUpdate">
            {{ t("checkUpdate") }}
          </button>
        </div>
      </div>
      <div class="card">
        <label>{{ t("language") }}</label>
        <div class="row">
          <button :class="{ active: locale === 'zh' }" @click="setLocale('zh')">{{ t("chinese") }}</button>
          <button :class="{ active: locale === 'en' }" @click="setLocale('en')">{{ t("english") }}</button>
        </div>
      </div>
    </template>
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
.tab-bar {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}
.tab-bar button {
  font-size: 13px;
  padding: 6px 12px;
}
.tab-bar button.active {
  background: var(--accent);
  color: #fff;
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
.toggle-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  color: var(--text);
  cursor: pointer;
}
.toggle-row input {
  width: auto;
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
