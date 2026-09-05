// In-memory Tauri mock — lets the Vue UI run in a plain browser (vite dev /
// preview) for GUI testing. Activated ONLY when the Tauri runtime is absent
// (no window.__TAURI_INTERNALS__); the packaged app never uses this.

type Args = Record<string, unknown> | undefined;

interface MockFriend {
  toxid: string;
  pubkey: string;
  name: string;
  avatar: string;
  bio: string;
  online: boolean;
  lastSeen: number | null;
}

const now = Date.now();
const pk = (s: string) => s.padEnd(64, "0").slice(0, 64);

const friends: MockFriend[] = [
  {
    toxid: pk("a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1") + "0001" + "aa",
    pubkey: pk("a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1"),
    name: "Alice（在线好友）",
    avatar: "",
    bio: "这是 Alice 的签名档",
    online: true,
    lastSeen: now,
  },
  {
    toxid: pk("b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2") + "0002" + "bb",
    pubkey: pk("b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2"),
    name: "Bob（离线好友）",
    avatar: "",
    bio: "",
    online: false,
    lastSeen: now - 3600_000,
  },
];

const ME = {
  toxid: pk("c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3") + "0003" + "cc",
  pubkey: pk("c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3"),
  name: "测试用户",
  statusMessage: "ToxSocial 测试模式",
  avatar: "",
  friendCount: friends.length,
};

let idSeq = 1000;
const nextId = () => ++idSeq;

interface Post {
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
  reactions: { emoji: string; count: number; mine: boolean }[];
  isOwn: boolean;
  tsVerified: boolean;
  attachment: string | null;
  source: string;
}

const posts: Post[] = [];
function addPost(p: Partial<Post>): Post {
  const full: Post = {
    id: "mock-" + nextId(),
    author: ME.pubkey,
    authorName: ME.name,
    authorAvatar: "",
    kind: "post",
    text: "",
    emoji: null,
    ts: Date.now(),
    parentId: null,
    commentCount: 0,
    reactionCount: 0,
    reactions: [],
    isOwn: true,
    tsVerified: true,
    attachment: null,
    source: "self",
    ...p,
  };
  posts.push(full);
  return full;
}

// Seed: a few own posts + friend posts + public fill for scroll pagination.
addPost({ text: "我的第一条公开帖子：**ToxSocial** 支持 Markdown！", ts: now - 1000 });
addPost({ text: "带附件的帖子示例", attachment: "示例文件.txt|1024", ts: now - 2000 });
addPost({
  author: friends[0].pubkey,
  authorName: friends[0].name,
  isOwn: false,
  text: "Alice 的帖子：大家好！",
  ts: now - 5000,
  tsVerified: true,
  source: "friend",
});
for (let i = 1; i <= 60; i++) {
  addPost({
    text: `历史公开帖 #${i}（用于测试无限滚动加载）`,
    ts: now - 10_000 - i * 60_000,
  });
}
// comments on the first own post
const first = posts[0];
posts.push({
  ...first,
  id: "mock-c1",
  kind: "comment",
  parentId: first.id,
  text: "第一条评论",
  ts: now + 1,
  reactions: [],
  isOwn: true,
  attachment: null,
});
posts.push({
  ...first,
  id: "mock-c2",
  kind: "comment",
  parentId: "mock-c1",
  text: "回复楼上的评论（嵌套）",
  ts: now + 2,
  reactions: [],
  isOwn: false,
  author: friends[0].pubkey,
  authorName: friends[0].name,
  attachment: null,
});
posts.push({
  ...first,
  id: "mock-r1",
  kind: "reaction",
  parentId: first.id,
  text: null,
  emoji: "👍",
  ts: now + 3,
  reactions: [],
  attachment: null,
  author: friends[0].pubkey,
  authorName: friends[0].name,
  isOwn: false,
});
first.commentCount = 2;
first.reactionCount = 1;
first.reactions = [{ emoji: "👍", count: 1, mine: false }];

function summarize(p: Post) {
  p.commentCount = posts.filter((x) => x.kind === "comment" && x.parentId === p.id).length;
  p.reactionCount = posts.filter((x) => x.kind === "reaction" && x.parentId === p.id).length;
  const counts = new Map<string, { count: number; mine: boolean }>();
  for (const r of posts.filter((x) => x.kind === "reaction" && x.parentId === p.id)) {
    const e = r.emoji || "👍";
    const cur = counts.get(e) || { count: 0, mine: false };
    cur.count++;
    cur.mine = cur.mine || r.isOwn;
    counts.set(e, cur);
  }
  p.reactions = Array.from(counts.entries()).map(([emoji, v]) => ({ emoji, ...v }));
}

function toTimeline(p: Post) {
  return { ...p };
}

const conferences = [
  { number: 0, id: pk("dddd"), name: "测试群组", peers: 1 },
  { number: 1, id: pk("eeee"), name: "公共群组示例", peers: 2 },
];
const channelMessages: Record<number, { id: number; peerName: string; text: string; ts: number; direction: number }[]> = {
  0: [
    { id: 1, peerName: "Alice（在线好友）", text: "群组里的第一条消息", ts: now - 60_000, direction: 0 },
    { id: 2, peerName: "", text: "我发的群组消息", ts: now - 30_000, direction: 1 },
  ],
  1: [],
};
const privateMsgs: { peer: string; id: number; text: string; ts: number; direction: number }[] = [];

const listeners = new Map<string, ((payload: unknown) => void)[]>();

function emitMock(event: string, payload: unknown) {
  for (const cb of listeners.get(event) || []) cb(payload);
}

function unhex(hex: string): string {
  return hex;
}

async function mockInvoke(cmd: string, args: Args = {}): Promise<unknown> {
  const a = args || {};
  switch (cmd) {
    case "get_own_info":
      return { ...ME };
    case "get_app_version":
      return "0.2.28";
    case "check_update":
      return { current: "0.2.28", latest: "0.2.28", hasUpdate: false };
    case "get_network_status":
      return {
        connected: true,
        connection: "udp",
        friends: friends.length,
        onlineFriends: friends.filter((f) => f.online).length,
        dhtNodes: 6,
        relayOk: true,
      };
    case "get_friends":
      return friends.map((f) => ({ ...f }));
    case "get_media_config":
      return { provider: "imgur", hasClientId: false };
    case "get_relay_url":
      return "https://toxsocial-relay.vcst.top";
    case "get_relay_urls":
      return ["https://toxsocial-relay.vcst.top"];
    case "set_relay_url":
    case "set_relay_urls":
    case "set_profile":
    case "set_avatar":
    case "set_avatar_url":
    case "set_auto_start":
    case "set_imgur_client_id":
      return null;
    case "get_auto_start":
      return false;
    case "file_transfers":
      return [];
    case "fetch_timeline": {
      const limit = (a.limit as number) || 50;
      return posts
        .filter((p) => p.kind === "post" && (p.isOwn || friends.some((f) => f.pubkey === p.author)))
        .sort((x, y) => y.ts - x.ts)
        .slice(0, limit)
        .map(toTimeline);
    }
    case "fetch_thread": {
      const id = a.postId as string;
      const root = posts.find((p) => p.id === id);
      if (!root) return [];
      // Recursive descendants, mirroring the real backend's recursive CTE.
      const descendants: Post[] = [];
      let frontier = [id];
      while (frontier.length) {
        const next = posts.filter((p) => frontier.includes(p.parentId || ""));
        descendants.push(...next);
        frontier = next.map((p) => p.id);
      }
      descendants.sort((x, y) => x.ts - y.ts);
      return [toTimeline(root), ...descendants.map(toTimeline)];
    }
    case "fetch_posts_by_author": {
      const pubkey = a.pubkey as string;
      return posts
        .filter((p) => p.kind === "post" && p.author === pubkey)
        .sort((x, y) => y.ts - x.ts)
        .slice(0, (a.limit as number) || 50)
        .map(toTimeline);
    }
    case "publish_post": {
      const p = addPost({
        text: (a.text as string) || "",
        isOwn: true,
        attachment: a.attachmentData ? `${a.attachmentName}|1024` : null,
      });
      summarize(p);
      // Simulate a friend reply shortly after publishing.
      const pid = p.id;
      setTimeout(() => {
        emitMock("feed:post", { id: pid, author: friends[0].pubkey, authorName: friends[0].name, text: p.text, ts: p.ts });
      }, 1500);
      return toTimeline(p);
    }
    case "publish_comment": {
      const c = addPost({
        kind: "comment",
        parentId: (a.postId as string) || "",
        text: (a.text as string) || "",
        ts: Date.now(),
      });
      const root = posts.find((p) => p.id === (a.postId as string));
      if (root) summarize(root);
      return toTimeline(c);
    }
    case "publish_reaction": {
      const r = addPost({
        kind: "reaction",
        parentId: (a.postId as string) || "",
        emoji: (a.emoji as string) || "👍",
        text: null,
        ts: Date.now(),
      });
      const root = posts.find((p) => p.id === (a.postId as string));
      if (root) summarize(root);
      return toTimeline(r);
    }
    case "fetch_public_timeline": {
      const limit = (a.limit as number) || 50;
      const before = a.before as number | undefined;
      let list = posts
        .filter((p) => p.kind === "post")
        .sort((x, y) => y.ts - x.ts);
      if (before) list = list.filter((p) => p.ts < before);
      return list.slice(0, limit).map(toTimeline);
    }
    case "fetch_relay_public_posts":
      return 0;
    case "request_public_posts":
      return 0;
    case "search_posts":
      return posts
        .filter((p) => p.kind === "post" && (p.text || "").includes((a.query as string) || ""))
        .map(toTimeline);
    case "search_directory":
    case "search_relay_directory":
      return [
        {
          name: "搜索到的人",
          pubkey: pk("e5e5"),
          toxid: pk("e5e5") + "0005" + "ee",
          avatar: "",
          relay: "",
          source: "relay",
        },
      ];
    case "add_friend":
      return nextId();
    case "remove_friend":
    case "remove_friend_by_toxid":
      return null;
    case "list_conferences":
      return conferences.map((c) => c.number);
    case "get_conference_id":
      return conferences.find((c) => c.number === a.conferenceNumber)?.id || "";
    case "get_conference_peer_count":
      return conferences.find((c) => c.number === a.conferenceNumber)?.peers || 1;
    case "conference_peers":
      return [
        { peerNumber: 0, name: "测试用户", publicKey: ME.pubkey },
        { peerNumber: 1, name: "Alice（在线好友）", publicKey: friends[0].pubkey },
      ];
    case "is_channel_owned":
      return true;
    case "channel_messages":
      return (channelMessages[(a.conferenceNumber as number) || 0] || []).slice(-(a.limit || 300));
    case "conference_send": {
      const n = (a.conferenceNumber as number) || 0;
      const peers = conferences.find((c) => c.number === n)?.peers || 1;
      const queued = peers <= 1;
      const id = nextId();
      (channelMessages[n] = channelMessages[n] || []).push({
        id,
        peerName: "",
        text: (a.text as string) || "",
        ts: Date.now(),
        direction: 1,
      });
      if (!queued) {
        setTimeout(() => {
          emitMock("channel:message", {
            conferenceNumber: n,
            channelId: conferences.find((c) => c.number === n)?.id || "",
            peerNumber: 1,
            peerName: "Alice（在线好友）",
            text: `收到：${a.text}`,
            id: nextId(),
            ts: Date.now(),
          });
        }, 1200);
      }
      return { id, queued };
    }
    case "conference_new":
      return 2;
    case "conference_delete":
      return null;
    case "list_public_channels":
      return [
        {
          name: "公共群组示例",
          desc: "示例公共群组（mock）",
          hostToxid: friends[0].toxid,
          channelId: pk("eeee"),
          hosts: [friends[0].pubkey],
          members: [friends[0].pubkey],
        },
      ];
    case "report_channel_memberships":
      return 1;
    case "register_public_channel":
    case "delete_public_channel":
    case "add_channel_host":
    case "remove_channel_host":
    case "send_join_channel":
    case "request_sync_all":
    case "request_directory_search":
      return 0;
    case "upload_media":
      return "https://example.com/mock-upload.png";
    case "send_private_message": {
      const peer = (a.peer as string) || "";
      const id = nextId();
      privateMsgs.push({ peer, id, text: (a.text as string) || "", ts: Date.now(), direction: 1 });
      // Simulate a friend reply.
      setTimeout(() => {
        const rid = nextId();
        const reply = `自动回复：${a.text}`;
        privateMsgs.push({ peer, id: rid, text: reply, ts: Date.now(), direction: 0 });
        emitMock("pm:message", { peer, authorName: "Alice（在线好友）", text: reply, id: rid, ts: Date.now() });
      }, 1000);
      return id;
    }
    case "create_community": {
      const id = pk(("comm" + (a.name || "")).slice(0, 20));
      return {
        channelId: id,
        conferenceNumber: 2,
        name: (a.name as string) || "community",
        desc: (a.desc as string) || "",
        createdByMe: true,
      };
    }
    case "my_communities":
      return [
        {
          channelId: pk("commmock"),
          conferenceNumber: 2,
          name: "测试社区",
          desc: "mock 社区",
          createdByMe: true,
        },
      ];
    case "join_community":
      return null;
    case "send_join_channel":
      return null;

    case "private_messages": {
      const peer = (a.peer as string) || "";
      return privateMsgs
        .filter((m) => m.peer === peer)
        .sort((x, y) => x.ts - y.ts)
        .slice(-(a.limit || 200))
        .map(({ id, text, ts, direction }) => ({ id, text, ts, direction }));
    }
    case "accept_file":
    case "reject_file":
    case "send_file_to_friend":
    case "send_file_to_friend_by_toxid":
      return nextId();
    case "request_attachment":
      return null;
    default:
      console.warn("[tauri-mock] unhandled command:", cmd);
      return null;
  }
}

function mockOnEvent<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  const cbs = listeners.get(event) || [];
  cbs.push(cb as (payload: unknown) => void);
  listeners.set(event, cbs);
  return Promise.resolve(() => {
    listeners.set(
      event,
      (listeners.get(event) || []).filter((x) => x !== cb),
    );
  });
}

let mockActive = false;

export function isMockMode(): boolean {
  return mockActive;
}

/** Install the invoke shims used by @tauri-apps/api when running in a browser. */
export function installTauriMock(): void {
  if (mockActive) return;
  if (typeof window === "undefined") return;
  // Real Tauri runtime present: never activate the mock.
  if ((window as unknown as Record<string, unknown>).__TAURI_INTERNALS__) return;
  mockActive = true;
  const w = window as unknown as Record<string, any>;
  console.info("[tauri-mock] active (browser GUI-test mode)");
  w.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: "mock" }, currentWebview: { label: "mock" } },
    plugins: {},
    invoke: (cmd: string, args?: Args) => mockInvoke(cmd, args),
    transformCallback: (callback: (res: unknown) => void) => {
      const id = nextId();
      w[`_${id}`] = callback;
      return id;
    },
    convertFileSrc: (p: string) => p,
  };
  // Expose helpers for debugging from the console.
  (w as any).__mockEmit = emitMock;
  void unhex;
}

export function mockInvokeShim(cmd: string, args?: Args): Promise<unknown> {
  return mockInvoke(cmd, args);
}

export function mockOnEventShim<T>(event: string, cb: (payload: T) => void): Promise<() => void> {
  return mockOnEvent<T>(event, cb);
}
