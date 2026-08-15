// ToxSocial optional directory + relay (Cloudflare Worker)
// Deploy on Cloudflare Workers free tier; no server needed.
//
// KV bindings:
//   DIRECTORY - key: pubkey -> JSON profile
//   OUTBOX    - key: `${pubkey}:${ts}:${id}` -> JSON post

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const path = url.pathname;

    if (path === '/api/directory' && request.method === 'GET') {
      const q = (url.searchParams.get('q') || '').toLowerCase();
      const list = await env.DIRECTORY.list();
      const items = [];
      for (const key of list.keys) {
        const val = await env.DIRECTORY.get(key.name, 'json');
        if (!val) continue;
        if (!q || (val.name || '').toLowerCase().includes(q) || val.pubkey.includes(q)) {
          items.push(val);
        }
      }
      return json({ items });
    }

    if (path === '/api/directory' && request.method === 'POST') {
      const body = await request.json();
      if (!body.pubkey) return json({ error: 'pubkey required' }, 400);
      const profile = {
        name: body.name || '',
        pubkey: body.pubkey,
        toxid: body.toxid || '',
        avatar: body.avatar || '',
        relay: body.relay || '',
        updated_at: Date.now(),
      };
      await env.DIRECTORY.put(body.pubkey, JSON.stringify(profile));
      return json({ ok: true });
    }

    if (path === '/api/outbox' && request.method === 'GET') {
      const pubkey = url.searchParams.get('pubkey');
      const since = Number(url.searchParams.get('since') || 0);
      if (!pubkey) return json({ error: 'pubkey required' }, 400);
      const prefix = `${pubkey}:`;
      const list = await env.OUTBOX.list({ prefix });
      const items = [];
      for (const key of list.keys) {
        const val = await env.OUTBOX.get(key.name, 'json');
        if (val && val.ts > since) items.push(val);
      }
      items.sort((a, b) => a.ts - b.ts);
      return json({ items });
    }

    if (path === '/api/outbox' && request.method === 'POST') {
      const body = await request.json();
      if (!body.pubkey || !body.id) return json({ error: 'pubkey and id required' }, 400);
      const key = `${body.pubkey}:${body.ts || Date.now()}:${body.id}`;
      await env.OUTBOX.put(key, JSON.stringify(body));
      return json({ ok: true });
    }

    return json({ error: 'not found' }, 404);
  },
};

function json(data, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}
