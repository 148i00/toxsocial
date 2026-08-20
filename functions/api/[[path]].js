// ToxSocial Relay - Cloudflare Pages Functions + D1

export async function onRequest(context) {
  const { request, env } = context;
  const url = new URL(request.url);
  const path = url.pathname;
  const db = env.toxsocial_db;
  if (!db) return json({ error: 'D1 binding not configured' }, 500);

  // Directory
  if (path === '/api/directory' && request.method === 'GET') {
    const q = (url.searchParams.get('q') || '').toLowerCase();
    let rows = await db.prepare('SELECT * FROM profiles').all();
    let items = rows.results || [];
    if (q) {
      items = items.filter((r) =>
        (r.name || '').toLowerCase().includes(q) || (r.pubkey || '').includes(q)
      );
    }
    return json({ items });
  }

  if (path === '/api/directory' && request.method === 'POST') {
    const body = await request.json();
    if (!body.pubkey) return json({ error: 'pubkey required' }, 400);
    await db.prepare(
      `INSERT INTO profiles (pubkey, name, toxid, avatar, relay, updated_at)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6)
       ON CONFLICT(pubkey) DO UPDATE SET
         name = excluded.name,
         toxid = excluded.toxid,
         avatar = excluded.avatar,
         relay = excluded.relay,
         updated_at = excluded.updated_at`
    ).bind(body.pubkey, body.name || '', body.toxid || '', body.avatar || '', body.relay || '', Date.now()).run();
    return json({ ok: true });
  }

  // Outbox
  if (path === '/api/outbox' && request.method === 'GET') {
    const pubkey = url.searchParams.get('pubkey');
    const since = Number(url.searchParams.get('since') || 0);
    let stmt = db.prepare('SELECT * FROM posts WHERE ts > ?1');
    if (pubkey) stmt = db.prepare('SELECT * FROM posts WHERE ts > ?1 AND pubkey = ?2');
    const result = pubkey
      ? await stmt.bind(since, pubkey).all()
      : await stmt.bind(since).all();
    let items = result.results || [];
    items.sort((a, b) => a.ts - b.ts);
    return json({ items });
  }

  if (path === '/api/outbox' && request.method === 'POST') {
    const body = await request.json();
    if (!body.pubkey || !body.id) return json({ error: 'pubkey and id required' }, 400);
    const pubkey = String(body.pubkey).toLowerCase();
    const sig = String(body.sig || '').toLowerCase();
    const edPk = String(body.edPk || '').toLowerCase();
    const text = String(body.text || '');
    // Ed25519 signature verification (anti-spoofing). The author's Tox
    // public key is an X25519 key; clients upload the matching Ed25519
    // public key (its birational image) and sign `id|pubkey|ts|text|true`.
    if (!/^[0-9a-f]{64}$/.test(pubkey) || !/^[0-9a-f]{128}$/.test(sig) || !/^[0-9a-f]{64}$/.test(edPk)) {
      return json({ error: 'missing or invalid signature fields (pubkey/sig/edPk)' }, 400);
    }
    const dataStr = `${body.id}|${pubkey}|${body.ts}|${text}|true`;
    const valid = await verifyPostSignature(pubkey, edPk, sig, dataStr);
    if (!valid) return json({ error: 'bad signature' }, 400);
    await db.prepare(
      `INSERT OR IGNORE INTO posts (id, pubkey, ts, text, sig)
       VALUES (?1, ?2, ?3, ?4, ?5)`
    ).bind(body.id, pubkey, body.ts || Date.now(), text, sig).run();
    return json({ ok: true });
  }

  // Channels
  if (path === '/api/channels' && request.method === 'GET') {
    const result = await db.prepare('SELECT * FROM channels').all();
    return json({ items: (result.results || []).map(parseChannel) });
  }

  if (path === '/api/channels' && request.method === 'POST') {
    const body = await request.json();
    if (!body.name || !body.hostToxid || !body.channelId) {
      return json({ error: 'name, hostToxid and channelId required' }, 400);
    }
    const hosts = body.hosts && body.hosts.length ? body.hosts : [body.hostToxid];
    await db.prepare(
      `INSERT INTO channels (channel_id, name, desc, host_toxid, hosts, updated_at)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6)
       ON CONFLICT(channel_id) DO UPDATE SET
         name = excluded.name,
         desc = excluded.desc,
         host_toxid = excluded.host_toxid,
         hosts = excluded.hosts,
         updated_at = excluded.updated_at`
    ).bind(body.channelId, body.name, body.desc || '', body.hostToxid, JSON.stringify(hosts), Date.now()).run();
    return json({ ok: true });
  }

  if (path === '/api/channels/hosts/add' && request.method === 'POST') {
    const body = await request.json();
    const { channelId, requesterToxid, newHostToxid } = body;
    if (!channelId || !requesterToxid || !newHostToxid) return json({ error: 'missing' }, 400);
    const row = await db.prepare('SELECT * FROM channels WHERE channel_id = ?1').bind(channelId).first();
    if (!row) return json({ error: 'channel not found' }, 404);
    const hosts = JSON.parse(row.hosts || '[]');
    if (!hosts.includes(requesterToxid)) return json({ error: 'not authorized' }, 403);
    if (!hosts.includes(newHostToxid)) hosts.push(newHostToxid);
    await db.prepare('UPDATE channels SET hosts = ?1 WHERE channel_id = ?2').bind(JSON.stringify(hosts), channelId).run();
    return json({ ok: true });
  }

  if (path === '/api/channels/hosts/remove' && request.method === 'POST') {
    const body = await request.json();
    const { channelId, requesterToxid, removeHostToxid } = body;
    if (!channelId || !requesterToxid || !removeHostToxid) return json({ error: 'missing' }, 400);
    const row = await db.prepare('SELECT * FROM channels WHERE channel_id = ?1').bind(channelId).first();
    if (!row) return json({ error: 'channel not found' }, 404);
    let hosts = JSON.parse(row.hosts || '[]');
    if (!hosts.includes(requesterToxid)) return json({ error: 'not authorized' }, 403);
    hosts = hosts.filter((h) => h !== removeHostToxid);
    if (hosts.length === 0) {
      await db.prepare('DELETE FROM channels WHERE channel_id = ?1').bind(channelId).run();
    } else {
      await db.prepare('UPDATE channels SET hosts = ?1 WHERE channel_id = ?2').bind(JSON.stringify(hosts), channelId).run();
    }
    return json({ ok: true });
  }

  if (path === '/api/channels/delete' && request.method === 'POST') {
    const body = await request.json();
    const { channelId, hostToxid } = body;
    if (!channelId || !hostToxid) return json({ error: 'missing' }, 400);
    const row = await db.prepare('SELECT * FROM channels WHERE channel_id = ?1').bind(channelId).first();
    if (!row) return json({ error: 'channel not found' }, 404);
    const hosts = JSON.parse(row.hosts || '[]');
    if (!hosts.includes(hostToxid)) return json({ error: 'not authorized' }, 403);
    await db.prepare('DELETE FROM channels WHERE channel_id = ?1').bind(channelId).run();
    return json({ ok: true });
  }

  return json({ error: 'not found' }, 404);
}

function parseChannel(row) {
  return {
    name: row.name,
    desc: row.desc,
    hostToxid: row.host_toxid,
    channelId: row.channel_id,
    hosts: JSON.parse(row.hosts || '[]'),
    updated_at: row.updated_at,
  };
}

// ---------------------------------------------------------------------------
// Ed25519 verification for public posts
// ---------------------------------------------------------------------------
// ToxSocial signs public posts by interpreting the Tox secret seed as an
// Ed25519 seed. The Edwards public key is the birational image of the Tox
// (X25519) public key: y = (u - 1) / (u + 1) mod p. Clients upload their true
// Ed25519 public key (`edPk`) together with the signature; here we verify
// 1) edPk maps back to the author's pubkey, and 2) the standard Ed25519
// signature holds (WebCrypto).

const ED25519_P = (1n << 255n) - 19n; // 2^255 - 19
const MODP_MASK = (1n << 255n) - 1n;

function modpow(base, exp, mod) {
  base %= mod;
  if (base < 0n) base += mod;
  let result = 1n;
  while (exp > 0n) {
    if (exp & 1n) result = (result * base) % mod;
    base = (base * base) % mod;
    exp >>= 1n;
  }
  return result;
}

function hexBytes(hex) {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return bytes;
}

// Ed25519/X25519 encodings are little-endian; parse hex accordingly.
function hexLeToBigInt(hex) {
  let le = "";
  for (let i = hex.length - 2; i >= 0; i -= 2) {
    le += hex.slice(i, i + 2);
  }
  return BigInt("0x" + le);
}

async function verifyPostSignature(pubkeyHex, edPkHex, sigHex, dataStr) {
  try {
    const key = await crypto.subtle.importKey(
      'raw', hexBytes(edPkHex), { name: 'Ed25519' }, false, ['verify'],
    );
    // Edwards y -> X25519 u = (1 + y) / (1 - y) mod p.
    const y = hexLeToBigInt(edPkHex) & MODP_MASK;
    if (y === 1n) return false; // (1 - y) == 0 -> no finite image
    const denom = modpow((1n - y) % ED25519_P, ED25519_P - 2n, ED25519_P);
    const u = (((1n + y) % ED25519_P) * denom) % ED25519_P;
    const wantU = hexLeToBigInt(pubkeyHex) & MODP_MASK;
    if (u !== wantU) return false; // edPk does not belong to this author
    const ok = await crypto.subtle.verify(
      { name: 'Ed25519' },
      key,
      hexBytes(sigHex),
      new TextEncoder().encode(dataStr),
    );
    return ok;
  } catch {
    return false;
  }
}

function json(data, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}
