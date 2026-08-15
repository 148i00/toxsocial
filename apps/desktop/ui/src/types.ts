export interface OwnInfo {
  toxid: string;
  pubkey: string;
  name: string;
  statusMessage: string;
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
  online: boolean;
  lastSeen: number | null;
}
