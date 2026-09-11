import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { locale } from "./i18n";
import { installTauriMock, isMockMode, mockInvokeShim, mockOnEventShim } from "./tauri-mock";
import type { ChannelMessageInfo, CommunityInfo, ConferencePeerInfo, ConferenceSendResult, DirectoryEntryInfo, FileTransferInfo, FriendInfo, MediaConfig, NetworkStatus, OwnInfo, PrivateMessageInfo, PublicChannelInfo, TimelineItem, UpdateInfo } from "./types";

// Browser GUI-test mode: route every call through the in-memory mock when the
// Tauri runtime is absent (the packaged app always has the runtime).
installTauriMock();
const inv = ((cmd: string, args?: Record<string, unknown>) =>
  isMockMode() ? (mockInvokeShim(cmd, args) as never) : (invoke(cmd, args as never) as never)) as typeof invoke;

export const api = {
  getOwnInfo: () => inv<OwnInfo>("get_own_info"),
  getAppVersion: () => inv<string>("get_app_version"),
  checkUpdate: () => inv<UpdateInfo>("check_update"),
  performUpdate: () => inv<void>("perform_update"),
  getNetworkStatus: () => inv<NetworkStatus>("get_network_status"),
  setProfile: (name: string, bio: string) => inv<void>("set_profile", { name, bio }),
  addFriend: (toxid: string, message: string, kind?: "friend" | "follow") =>
    inv<number>("add_friend", { toxid, message, kind }),
  setContactKind: (toxid: string, kind: "friend" | "follow") =>
    inv<void>("set_contact_kind", { toxid, kind }),
  removeFriend: (friendNumber: number) => inv<void>("remove_friend", { friendNumber }),
  removeFriendByToxid: (toxid: string) => inv<void>("remove_friend_by_toxid", { toxid }),
  publishPost: (text: string, isPublic?: boolean, community?: string) =>
    inv<TimelineItem>("publish_post", { text, public: isPublic, community }),
  fileTransfers: () => inv<FileTransferInfo[]>("file_transfers"),
  publishComment: (postId: string, text: string, replyTo?: string) =>
    inv<TimelineItem>("publish_comment", { postId, text, replyTo }),
  publishReaction: (postId: string, emoji: string) =>
    inv<TimelineItem>("publish_reaction", { postId, emoji }),
  fetchTimeline: (limit?: number) => inv<TimelineItem[]>("fetch_timeline", { limit }),
  fetchThread: (postId: string) => inv<TimelineItem[]>("fetch_thread", { postId }),
  fetchPostsByAuthor: (pubkey: string, limit?: number) =>
    inv<TimelineItem[]>("fetch_posts_by_author", { pubkey, limit }),
  getFriends: () => inv<FriendInfo[]>("get_friends"),
  uploadMedia: (dataBase64: string, filename: string) =>
    inv<string>("upload_media", { dataBase64, filename }),
  sendFileToFriend: (friendNumber: number, filename: string, dataBase64: string) =>
    inv<number>("send_file_to_friend", { friendNumber, filename, dataBase64 }),
  sendFileToFriendByToxid: (toxid: string, filename: string, dataBase64: string) =>
    inv<number>("send_file_to_friend_by_toxid", { toxid, filename, dataBase64 }),
  acceptFile: (friendNumber: number, fileNumber: number) =>
    inv<void>("accept_file", { friendNumber, fileNumber }),
  rejectFile: (friendNumber: number, fileNumber: number) =>
    inv<void>("reject_file", { friendNumber, fileNumber }),
  sendJoinChannel: (toxid: string, channelId: string) =>
    inv<void>("send_join_channel", { toxid, channelId }),
  setAvatar: (dataBase64: string) => inv<string>("set_avatar", { dataBase64 }),
  setAvatarUrl: (url: string) => inv<void>("set_avatar_url", { url }),
  setImgurClientId: (clientId: string) => inv<void>("set_imgur_client_id", { clientId }),
  getMediaConfig: () => inv<MediaConfig>("get_media_config"),
  getRelayUrl: () => inv<string>("get_relay_url"),
  setRelayUrl: (url: string) => inv<void>("set_relay_url", { url }),
  getRelayUrls: () => inv<string[]>("get_relay_urls"),
  setRelayUrls: (urls: string[]) => inv<void>("set_relay_urls", { urls }),
  getAutoStart: () => inv<boolean>("get_auto_start"),
  setAutoStart: (enabled: boolean) => inv<void>("set_auto_start", { enabled }),
  conferenceNew: () => inv<number>("conference_new"),
  isChannelOwned: (conferenceNumber: number) =>
    inv<boolean>("is_channel_owned", { conferenceNumber }),
  conferenceDelete: (conferenceNumber: number) =>
    inv<void>("conference_delete", { conferenceNumber }),
  conferenceInvite: (friendNumber: number, conferenceNumber: number) =>
    inv<void>("conference_invite", { friendNumber, conferenceNumber }),
  conferenceInviteByToxid: (conferenceNumber: number, toxid: string) =>
    inv<void>("conference_invite_by_toxid", { conferenceNumber, toxid }),
  conferenceSend: (conferenceNumber: number, text: string) =>
    inv<ConferenceSendResult>("conference_send", { conferenceNumber, text }),
  channelMessages: (conferenceNumber: number, limit?: number, beforeId?: number) =>
    inv<ChannelMessageInfo[]>("channel_messages", { conferenceNumber, limit, beforeId }),
  conferencePeers: (conferenceNumber: number) =>
    inv<ConferencePeerInfo[]>("conference_peers", { conferenceNumber }),
  getConferenceId: (conferenceNumber: number) =>
    inv<string>("get_conference_id", { conferenceNumber }),
  getConferencePeerCount: (conferenceNumber: number) =>
    inv<number>("get_conference_peer_count", { conferenceNumber }),
  listConferences: () => inv<number[]>("list_conferences"),
  requestSyncAll: () => inv<number>("request_sync_all"),
  searchPosts: (query: string, limit?: number) =>
    inv<TimelineItem[]>("search_posts", { query, limit }),
  searchDirectory: (query: string, limit?: number) =>
    inv<DirectoryEntryInfo[]>("search_directory", { query, limit }),
  requestDirectorySearch: (query: string, depth?: number) =>
    inv<number>("request_directory_search", { query, depth }),
  fetchPublicTimeline: (limit?: number, before?: number) =>
    inv<TimelineItem[]>("fetch_public_timeline", { limit, before }),
  fetchCommunityTimeline: (community: string, limit?: number, before?: number) =>
    inv<TimelineItem[]>("fetch_public_timeline", { community, limit, before }),
  requestPublicPosts: (since?: number, depth?: number) =>
    inv<number>("request_public_posts", { since, depth }),
  searchRelayDirectory: (query: string) =>
    inv<DirectoryEntryInfo[]>("search_relay_directory", { query }),
  fetchRelayPublicPosts: (since?: number, community?: string) =>
    inv<number>("fetch_relay_public_posts", { since, community }),
  listPublicChannels: () => inv<PublicChannelInfo[]>("list_public_channels"),
  reportChannelMemberships: () => inv<number>("report_channel_memberships"),
  registerPublicChannel: (conferenceNumber: number, name: string, desc: string) =>
    inv<void>("register_public_channel", { conferenceNumber, name, desc }),
  deletePublicChannel: (channelId: string) =>
    inv<void>("delete_public_channel", { channelId }),
  addChannelHost: (channelId: string, newHostToxid: string) =>
    inv<void>("add_channel_host", { channelId, newHostToxid }),
  removeChannelHost: (channelId: string, removeHostToxid: string) =>
    inv<void>("remove_channel_host", { channelId, removeHostToxid }),
  sendPrivateMessage: (peer: string, text: string) =>
    inv<number>("send_private_message", { peer, text }),
  privateMessages: (peer: string, limit?: number) =>
    inv<PrivateMessageInfo[]>("private_messages", { peer, limit }),
  createCommunity: (name: string, desc: string) =>
    inv<CommunityInfo>("create_community", { name, desc }),
  myCommunities: () => inv<CommunityInfo[]>("my_communities"),
  joinCommunity: (channelId: string, name: string, desc: string) =>
    inv<void>("join_community", { channelId, name, desc }),
  updateCommunityConferences: (
    entries: { channelId: string; conferenceNumber: number }[],
  ) => inv<void>("update_community_conferences", { entries }),
  cleanupDatabase: () =>
    inv<{ removedPosts: number; removedChannelMsgs: number; removedPrivateMsgs: number; dbSizeBytes: number }>("cleanup_database"),
  dbStats: () =>
    inv<{ dbSizeBytes: number; postCount: number; channelMsgCount: number; privateMsgCount: number }>("db_stats"),
  exportAccount: () => inv<string>("export_account"),
  importAccount: (dataB64: string) => inv<void>("import_account", { dataB64 }),
};

export function onEvent<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  if (isMockMode()) return mockOnEventShim<T>(event, cb);
  return listen<T>(event, (e) => cb(e.payload));
}

export function formatTime(ts: number): string {
  const d = new Date(ts);
  const now = Date.now();
  const diff = now - ts;
  if (locale.value === "zh") {
    if (diff < 60_000) return "刚刚";
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
    if (diff < 7 * 86_400_000) return `${Math.floor(diff / 86_400_000)} 天前`;
  } else {
    if (diff < 60_000) return "just now";
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} minutes ago`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} hours ago`;
    if (diff < 7 * 86_400_000) return `${Math.floor(diff / 86_400_000)} days ago`;
  }
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate(),
  ).padStart(2, "0")}`;
}
