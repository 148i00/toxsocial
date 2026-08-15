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
  name: string;
  avatar: string;
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
