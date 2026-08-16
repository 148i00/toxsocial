import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ConferencePeerInfo, DirectoryEntryInfo, FriendInfo, MediaConfig, OwnInfo, PublicChannelInfo, TimelineItem } from "./types";

export const api = {
  getOwnInfo: () => invoke<OwnInfo>("get_own_info"),
  setProfile: (name: string, bio: string) => invoke<void>("set_profile", { name, bio }),
  addFriend: (toxid: string, message: string) => invoke<number>("add_friend", { toxid, message }),
  removeFriend: (friendNumber: number) => invoke<void>("remove_friend", { friendNumber }),
  removeFriendByToxid: (toxid: string) => invoke<void>("remove_friend_by_toxid", { toxid }),
  publishPost: (text: string, isPublic?: boolean) =>
    invoke<TimelineItem>("publish_post", { text, public: isPublic }),
  publishComment: (postId: string, text: string) =>
    invoke<TimelineItem>("publish_comment", { postId, text }),
  publishReaction: (postId: string, emoji: string) =>
    invoke<TimelineItem>("publish_reaction", { postId, emoji }),
  fetchTimeline: (limit?: number) => invoke<TimelineItem[]>("fetch_timeline", { limit }),
  fetchThread: (postId: string) => invoke<TimelineItem[]>("fetch_thread", { postId }),
  getFriends: () => invoke<FriendInfo[]>("get_friends"),
  uploadMedia: (dataBase64: string, filename: string) =>
    invoke<string>("upload_media", { dataBase64, filename }),
  setAvatar: (dataBase64: string) => invoke<string>("set_avatar", { dataBase64 }),
  setAvatarUrl: (url: string) => invoke<void>("set_avatar_url", { url }),
  setImgurClientId: (clientId: string) => invoke<void>("set_imgur_client_id", { clientId }),
  getMediaConfig: () => invoke<MediaConfig>("get_media_config"),
  conferenceNew: () => invoke<number>("conference_new"),
  conferenceInvite: (friendNumber: number, conferenceNumber: number) =>
    invoke<void>("conference_invite", { friendNumber, conferenceNumber }),
  conferenceInviteByToxid: (conferenceNumber: number, toxid: string) =>
    invoke<void>("conference_invite_by_toxid", { conferenceNumber, toxid }),
  conferenceSend: (conferenceNumber: number, text: string) =>
    invoke<void>("conference_send", { conferenceNumber, text }),
  conferencePeers: (conferenceNumber: number) =>
    invoke<ConferencePeerInfo[]>("conference_peers", { conferenceNumber }),
  getConferenceId: (conferenceNumber: number) =>
    invoke<string>("get_conference_id", { conferenceNumber }),
  getConferencePeerCount: (conferenceNumber: number) =>
    invoke<number>("get_conference_peer_count", { conferenceNumber }),
  listConferences: () => invoke<number[]>("list_conferences"),
  requestSyncAll: () => invoke<number>("request_sync_all"),
  searchPosts: (query: string, limit?: number) =>
    invoke<TimelineItem[]>("search_posts", { query, limit }),
  searchDirectory: (query: string, limit?: number) =>
    invoke<DirectoryEntryInfo[]>("search_directory", { query, limit }),
  requestDirectorySearch: (query: string, depth?: number) =>
    invoke<number>("request_directory_search", { query, depth }),
  fetchPublicTimeline: (limit?: number) =>
    invoke<TimelineItem[]>("fetch_public_timeline", { limit }),
  requestPublicPosts: (since?: number, depth?: number) =>
    invoke<number>("request_public_posts", { since, depth }),
  searchRelayDirectory: (query: string) =>
    invoke<DirectoryEntryInfo[]>("search_relay_directory", { query }),
  fetchRelayPublicPosts: (since?: number) =>
    invoke<number>("fetch_relay_public_posts", { since }),
  listPublicChannels: () => invoke<PublicChannelInfo[]>("list_public_channels"),
  registerPublicChannel: (conferenceNumber: number, name: string, desc: string) =>
    invoke<void>("register_public_channel", { conferenceNumber, name, desc }),
};

export function onEvent<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => cb(e.payload));
}

export function formatTime(ts: number): string {
  const d = new Date(ts);
  const now = Date.now();
  const diff = now - ts;
  if (diff < 60_000) return "刚刚";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
  if (diff < 7 * 86_400_000) return `${Math.floor(diff / 86_400_000)} 天前`;
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate(),
  ).padStart(2, "0")}`;
}
