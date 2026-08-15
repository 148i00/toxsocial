// Minimal i18n for ToxSocial UI.
import { ref } from "vue";

export type Locale = "zh" | "en";

const messages: Record<Locale, Record<string, string>> = {
  zh: {
    home: "首页",
    friends: "关注",
    channels: "频道",
    settings: "设置",
    notifications: "通知",
    connectedDht: "已连接 DHT",
    searchPlaceholder: "搜索帖子…",
    noSearchResults: "没有匹配的帖子",
    searching: "搜索中…",
    emptyTimeline: "时间线还是空的 — 添加好友并等待对方发帖吧。",
    loading: "加载中…",
    publish: "发布",
    sending: "发送中…",
    imageVideo: "图片/视频",
    uploading: "上传中…",
    composerHint: "Ctrl+Enter 发送 · 上限 50000 字符 · 支持 Markdown 与长文分片",
    composerPlaceholder: "分享你的想法…（端到端加密广播给所有好友，支持 Markdown 和长文自动分片）",
    friendsTitle: "关注管理",
    addFriendPlaceholder: "粘贴好友的 ToxID（76 位十六进制）",
    addFriendMsgPlaceholder: "好友请求附言",
    add: "添加",
    noFriends: "还没有好友。把上方输入框换成你的 ToxID 发给别人，或粘贴对方的 ToxID 添加。",
    unfollow: "取关",
    online: "在线",
    offline: "离线",
    channelsTitle: "频道",
    createChannel: "创建频道",
    inviteFriend: "邀请好友（好友编号）",
    sendMessage: "发送消息",
    send: "发送",
    settingsTitle: "设置",
    save: "保存",
  },
  en: {
    home: "Home",
    friends: "Following",
    channels: "Channels",
    settings: "Settings",
    notifications: "Notifications",
    connectedDht: "Connected to DHT",
    searchPlaceholder: "Search posts…",
    noSearchResults: "No matching posts",
    searching: "Searching…",
    emptyTimeline: "Timeline is empty — add friends and wait for their posts.",
    loading: "Loading…",
    publish: "Publish",
    sending: "Sending…",
    imageVideo: "Image/Video",
    uploading: "Uploading…",
    composerHint: "Ctrl+Enter to send · up to 50000 chars · Markdown & long-post split supported",
    composerPlaceholder: "Share your thoughts… (E2E encrypted broadcast to all friends, Markdown and long-post split supported)",
    friendsTitle: "Following",
    addFriendPlaceholder: "Paste friend ToxID (76 hex chars)",
    addFriendMsgPlaceholder: "Friend request message",
    add: "Add",
    noFriends: "No friends yet. Share your ToxID or paste another ToxID to add.",
    unfollow: "Unfollow",
    online: "Online",
    offline: "Offline",
    channelsTitle: "Channels",
    createChannel: "Create Channel",
    inviteFriend: "Invite friend (friend #)",
    sendMessage: "Send message",
    send: "Send",
    settingsTitle: "Settings",
    save: "Save",
  },
};

export const locale = ref<Locale>(
  (localStorage.getItem("toxsocial_locale") as Locale) || "zh",
);

export function t(key: string): string {
  return messages[locale.value][key] || key;
}

export function setLocale(next: Locale) {
  locale.value = next;
  localStorage.setItem("toxsocial_locale", next);
}
