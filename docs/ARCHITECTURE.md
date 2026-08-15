# ToxSocial — 架构文档

## 1. 工作区结构（cargo workspace）

```
toxproject/
├── Cargo.toml                 # [workspace] members
├── docs/
│   ├── PLAN.md                # 总体规划（本系列入口）
│   ├── ARCHITECTURE.md        # 本文档
│   └── PROTOCOL.md            # 社交协议规范
├── third_party/
│   └── c-toxcore/             # git submodule（官方仓库）
├── crates/
│   ├── tox-ffi/               # bindgen 绑定（bindings.rs 检入）+ link 静态库
│   ├── tox-core/              # 安全封装：ToxSession、事件循环、回调→channel
│   ├── tox-social/            # 社交协议：envelope 编解码、feed 引擎、回补逻辑
│   ├── tox-store/             # SQLite：schema、读写、迁移
│   └── tox-cli/               # 命令行客户端（开发/联调/冒烟测试）
└── apps/
    └── desktop/               # Tauri v2 + Vue3 + TS 前端
```

依赖方向（单向，禁止反向）：

```
tox-cli ─┐
         ├─► tox-social ─► tox-core ─► tox-ffi ─► c-toxcore
desktop ─┘      │
                └─► tox-store（rusqlite）
```

## 2. 线程与事件模型

Tox 是轮询模型（`tox_iterate`，建议 50ms tick），所有回调在主事件循环线程触发。
设计原则：**回调里绝不做阻塞操作，只转事件**。

```
┌─ tox 事件线程 ────────────────────────────┐
│ loop { sleep(interval); tox_iterate(tx) } │   ← C 回调在此线程执行
│   callbacks ──► mpsc::Sender<Event>        │
└───────────────────┬───────────────────────┘
                    │ Event（friend_message / friend_request /
                    │  friend_status / file_* / conference_* …）
                    ▼
┌─ tokio runtime（Tauri 共享）───────────────┐
│ tox-social::FeedEngine：                 │
│   1. 解析 envelope（serde_json）          │
│   2. 校验（来源必须是直接好友）            │
│   3. 写 tox-store（幂等去重 by id）       │
│   4. 生成 UI 事件 → Tauri emit("feed:…") │
│ Tauri commands（发帖/评论/关注等）        │
└───────────────────┬───────────────────────┘
                    ▼
┌─ Tauri 前端（Vue3）──────────────────────┐
│ listen("feed:post") → 更新时间线/评论流    │
└──────────────────────────────────────────┘
```

## 3. 核心数据流

### 3.1 发帖流程
```
前端发帖框 ──invoke("publish_post", text)──► command
  → tox-social::post(text)
    → 生成 Post { id: uuid, ts: now }
    → tox-store 写入自己的帖子
    → 对每个好友 tox_core::send_friend_message(friend, envelope(post))
    → Tauri emit("feed:post") 刷新本地时间线
```

### 3.2 收帖流程
```
tox 事件线程：friend_message 回调
  → Event::FriendMessage { friend, text }
  → FeedEngine：
      if 前缀 != "TSP/1 " → 忽略（普通聊天消息留给后续 IM 功能）
      envelope 解析 → Post/Comment/Reaction
      校验 author == friend 的公钥（Tox 层已保证是好友直接发来的，防伪造）
      tox-store upsert（id 唯一索引，幂等）
      → Tauri emit 对应事件
```

### 3.3 离线回补（M4，pull 模式）
```
好友上线（friend_status → ONLINE）
  → 发送 sync_req { since: 本地已有该好友帖子的最新 ts }
  → 对方回 sync_posts + 帖子流（按时间）
  → 本地按 id 去重入库，emit 事件
```

## 4. SQLite Schema（tox-store v1）

```sql
-- 身份与配置
CREATE TABLE kv (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);                                     -- tox_save 数据、昵称、设置等

-- 好友（= 关注）
CREATE TABLE friends (
  toxid       TEXT PRIMARY KEY,        -- 64 hex 公钥
  nospam      TEXT,                    -- 4B nospam（仅添加时用）
  name        TEXT,
  status      INTEGER,                 -- 0 离线 1 在线 2 拉黑
  added_at    INTEGER NOT NULL,
  last_seen   INTEGER
);

-- 帖子/评论/反应（统一表，parent 关联）
CREATE TABLE posts (
  id           TEXT PRIMARY KEY,       -- uuid
  author       TEXT NOT NULL,          -- 64 hex 公钥
  kind         INTEGER NOT NULL,       -- 0 post 1 comment 2 reaction
  parent_id    TEXT,                   -- 评论/反应指向的帖子
  text         TEXT,
  emoji        TEXT,                   -- reaction 用
  ts           INTEGER NOT NULL,       -- 作者时钟（排序用）
  received_at  INTEGER NOT NULL,       -- 本地接收时间
  source       INTEGER NOT NULL,       -- 0 自己发布 1 好友直发 2 频道
  channel_id   TEXT,                   -- 频道（conference）id
  UNIQUE(id, author)
);
CREATE INDEX idx_posts_author ON posts(author);
CREATE INDEX idx_posts_ts     ON posts(ts DESC);
CREATE INDEX idx_posts_parent ON posts(parent_id);
```

时间线查询：`WHERE author IN (关注列表) ORDER BY ts DESC LIMIT ?`；
帖子详情：`SELECT * FROM posts WHERE id=?` + 子评论 `WHERE parent_id=? ORDER BY ts`。

## 5. 关键 Rust 类型草案

```rust
// tox-ffi：仅 unsafe 绑定
pub mod bindings;   // bindgen 输出，检入仓库

// tox-core
pub struct ToxSession { /* *mut Tox, save_data 管理, 事件发送端 */ }
pub enum Event {
    FriendRequest { public_key: [u8; 32], message: Vec<u8> },
    FriendMessage { friend: FriendId, text: String },
    FriendStatus  { friend: FriendId, status: Status },
    FileRecv      { friend: FriendId, file_id: u32, kind: FileKind, size: u64 },
    ConferenceMessage { conference: u32, peer: PeerId, text: String },
    // ...
}

// tox-social（协议层）
#[derive(Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Envelope {
    Post(Post),
    Comment(Comment),
    Reaction(Reaction),
    Profile(Profile),
    SyncReq { since: i64 },
    SyncPosts { posts: Vec<Post> },
    // M4: FileMeta { file_id, name, size, sha256 } …
}

// tox-store
pub struct Store { conn: rusqlite::Connection }
impl Store {
    pub fn upsert_post(&self, p: &Post) -> Result<bool>;   // false=重复
    pub fn timeline(&self, friends: &[String], limit: u32) -> Result<Vec<Post>>;
    // …
}
```

## 6. 与 Tauri 的集成

- `apps/desktop/src-tauri/`：Rust crate，依赖 workspace 内 crates；
- 前端目录 `apps/desktop/ui/`：Vue3 + Vite + Pinia；
- Tauri v2 事件：后端 `app.emit("feed:post", payload)`，前端 `listen(...)`；
- 命令：`publish_post` / `fetch_timeline` / `fetch_post` / `add_friend` / `accept_request` / `publish_comment` / `set_profile` / `get_own_toxid` / `get_friends` / `join_channel` …；
- 生命周期：Tauri `setup` 时启动 ToxSession 线程，`on_window_event(CloseRequested)` 时保存并退出。
