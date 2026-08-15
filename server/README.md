# ToxSocial 可选目录 / Relay（Cloudflare Worker）

如果你没有自己的服务器，可以用 **Cloudflare Workers 免费版** 部署一个轻量目录和 Relay。

## 功能
- `POST /api/directory`：注册公开资料
- `GET /api/directory?q=...`：搜索用户
- `POST /api/outbox`：发布公开帖子
- `GET /api/outbox?pubkey=...&since=...`：拉取公开帖子

## 部署步骤
1. 注册 Cloudflare 账号（免费）
2. 安装 Wrangler：
   ```bash
   npm install -g wrangler
   ```
3. 登录：
   ```bash
   wrangler login
   ```
4. 在 `server/` 目录创建 KV namespace：
   ```bash
   wrangler kv:namespace create DIRECTORY
   wrangler kv:namespace create OUTBOX
   ```
5. 把返回的 id 填到 `wrangler.toml`（参考下面）
6. 部署：
   ```bash
   wrangler deploy
   ```

## wrangler.toml 示例
```toml
name = "toxsocial-relay"
main = "worker.js"
compatibility_date = "2025-01-01"

[[kv_namespaces]]
binding = "DIRECTORY"
id = "你的DIRECTORY_KV_ID"

[[kv_namespaces]]
binding = "OUTBOX"
id = "你的OUTBOX_KV_ID"
```

## 说明
- 这只是**可选增强**
- 没有它，ToxSocial 也能通过好友网络进行 P2P 目录查找和公开内容分发
- 有它之后，搜索和发现会更快、更稳定
