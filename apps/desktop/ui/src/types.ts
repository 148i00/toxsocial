export interface NetworkStatus {
  connected: boolean;
  connection: string;
  friends: number;
  onlineFriends: number;
  dhtNodes: number;
  relayOk: boolean;
}

export interface OwnInfo {
  toxid: string;
  pubkey: string;
  name: string;
  statusMessage: string;
  avatar: string;
  friendCount: number;
}

export interface ReactionSummary {
  emoji: string;
  count: number;
}

export interface TimelineItem {
  id: string;
  author: string;
  authorName: string;
  authorAvatar: string;
  kind: "post" | "comment" | "reaction";
  text: string | null;
  emoji: string | null;
  ts: number;
  parentId: string | null;
  commentCount: number;
  reactionCount: number;
  reactions: ReactionSummary[];
  isOwn: boolean;
  source: string;
}

export interface FriendInfo {
  toxid: string;
  pubkey: string;
  name: string;
  avatar: string;
  bio: string;
  online: boolean;
  lastSeen: number | null;
}

export interface MediaConfig {
  provider: string;
  hasClientId: boolean;
}

export interface ConferencePeerInfo {
  peerNumber: number;
  name: string;
  publicKey: string;
}

export interface DirectoryEntryInfo {
  name: string;
  pubkey: string;
  toxid: string;
  avatar: string;
  relay: string;
  source: string;
}

export interface PublicChannelInfo {
  name: string;
  desc: string;
  hostToxid: string;
  channelId: string;
  hosts: string[];
}
