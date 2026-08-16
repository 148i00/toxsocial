// ToxSocial optional directory + relay (Cloudflare Worker)
// Uses a single KV key per collection to avoid daily list-operation limits.
//
// KV bindings:
//   DIRECTORY - key "all" -> JSON array of profiles
//   OUTBOX    - key "all" -> JSON array of posts
//   CHANNELS  - key "all" -> JSON array of channels

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const path = url.pathname;

    if (path === '/api/directory' && request.method === 'GET') {
      const q = (url.searchParams.get('q') || '').toLowerCase();
      const all = (await env.DIRECTORY.get('all', 'json')) || [];
      const items = all.filter((val) => {
        if (!q) return true;
        return (val.name || '').toLowerCase().includes(q) || val.pubkey.includes(q);
      });
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
      const all = (await env.DIRECTORY.get('all', 'json')) || [];
      const idx = all.findIndex((x) => x.pubkey === profile.pubkey);
      if (idx >= 0) all[idx] = profile; else all.push(profile);
      await env.DIRECTORY.put('all', JSON.stringify(all));
      return json({ ok: true });
    }

    if (path === '/api/outbox' && request.method === 'GET') {
      const pubkey = url.searchParams.get('pubkey');
      const since = Number(url.searchParams.get('since') || 0);
      const all = (await env.OUTBOX.get('all', 'json')) || [];
      let items = all.filter((x) => x.ts > since);
      if (pubkey) items = items.filter((x) => x.pubkey === pubkey);
      items.sort((a, b) => a.ts - b.ts);
      return json({ items });
    }

    if (path === '/api/outbox' && request.method === 'POST') {
      const body = await request.json();
      if (!body.pubkey || !body.id) return json({ error: 'pubkey and id required' }, 400);
      const all = (await env.OUTBOX.get('all', 'json')) || [];
      if (!all.some((x) => x.id === body.id && x.pubkey === body.pubkey)) {
        all.push({
          pubkey: body.pubkey,
          id: body.id,
          ts: body.ts || Date.now(),
          text: body.text || '',
          sig: body.sig || '',
          type: body.type || 'post',
        });
        await env.OUTBOX.put('all', JSON.stringify(all));
      }
      return json({ ok: true });
    }

    if (path === '/api/channels' && request.method === 'GET') {
      const all = (await env.CHANNELS.get('all', 'json')) || [];
      return json({ items: all.map(withActiveMembers) });
    }

    if (path === '/api/channels' && request.method === 'POST') {
      const body = await request.json();
      if (!body.name || !body.hostToxid || !body.channelId) {
        return json({ error: 'name, hostToxid and channelId required' }, 400);
      }
      const all = (await env.CHANNELS.get('all', 'json')) || [];
      const channel = {
        name: body.name,
        desc: body.desc || '',
        hostToxid: body.hostToxid,
        hosts: body.hosts && body.hosts.length ? body.hosts : [body.hostToxid],
        members: body.members && body.members.length
          ? body.members.map((m) => ({ toxid: m, ts: Date.now() }))
          : [{ toxid: body.hostToxid, ts: Date.now() }],
        channelId: body.channelId,
        updated_at: Date.now(),
      };
      const idx = all.findIndex((x) => x.channelId === channel.channelId);
      if (idx >= 0) all[idx] = channel; else all.push(channel);
      await env.CHANNELS.put('all', JSON.stringify(all));
      return json({ ok: true });
    }

    if (path === '/api/channels/hosts/add' && request.method === 'POST') {
      const body = await request.json();
      const { channelId, requesterToxid, newHostToxid } = body;
      if (!channelId || !requesterToxid || !newHostToxid) {
        return json({ error: 'channelId, requesterToxid and newHostToxid required' }, 400);
      }
      const all = (await env.CHANNELS.get('all', 'json')) || [];
      const ch = all.find((x) => x.channelId === channelId);
      if (!ch) return json({ error: 'channel not found' }, 404);
      const hosts = ch.hosts || [ch.hostToxid];
      if (!hosts.includes(requesterToxid)) return json({ error: 'not authorized' }, 403);
      if (!hosts.includes(newHostToxid)) hosts.push(newHostToxid);
      ch.hosts = hosts;
      await env.CHANNELS.put('all', JSON.stringify(all));
      return json({ ok: true });
    }

    if (path === '/api/channels/hosts/remove' && request.method === 'POST') {
      const body = await request.json();
      const { channelId, requesterToxid, removeHostToxid } = body;
      if (!channelId || !requesterToxid || !removeHostToxid) {
        return json({ error: 'channelId, requesterToxid and removeHostToxid required' }, 400);
      }
      const all = (await env.CHANNELS.get('all', 'json')) || [];
      const ch = all.find((x) => x.channelId === channelId);
      if (!ch) return json({ error: 'channel not found' }, 404);
      const hosts = ch.hosts || [ch.hostToxid];
      if (!hosts.includes(requesterToxid)) return json({ error: 'not authorized' }, 403);
      ch.hosts = hosts.filter((h) => h !== removeHostToxid);
      if (ch.hosts.length === 0) {
        const idx = all.findIndex((x) => x.channelId === channelId);
        if (idx >= 0) all.splice(idx, 1);
      }
      await env.CHANNELS.put('all', JSON.stringify(all));
      return json({ ok: true });
    }

    if (path === '/api/channels/members/report' && request.method === 'POST') {
      const body = await request.json();
      const { channelId, memberToxid } = body;
      if (!channelId || !memberToxid) {
        return json({ error: 'channelId and memberToxid required' }, 400);
      }
      const all = (await env.CHANNELS.get('all', 'json')) || [];
      const ch = all.find((x) => x.channelId === channelId);
      if (!ch) return json({ error: 'channel not found' }, 404);
      let members = ch.members || [];
      members = members.filter((m) => m.toxid !== memberToxid);
      members.push({ toxid: memberToxid, ts: Date.now() });
      if (members.length > 500) members = members.slice(-500);
      ch.members = members;
      await env.CHANNELS.put('all', JSON.stringify(all));
      return json({ ok: true });
    }

    if (path === '/api/channels/delete' && request.method === 'POST') {
      const body = await request.json();
      const channelId = body.channelId;
      const hostToxid = body.hostToxid;
      if (!channelId || !hostToxid) return json({ error: 'channelId and hostToxid required' }, 400);
      const all = (await env.CHANNELS.get('all', 'json')) || [];
      const ch = all.find((x) => x.channelId === channelId);
      if (!ch) return json({ error: 'channel not found' }, 404);
      const hosts = ch.hosts || [ch.hostToxid];
      if (!hosts.includes(hostToxid)) return json({ error: 'not authorized' }, 403);
      const idx = all.findIndex((x) => x.channelId === channelId);
      if (idx >= 0) all.splice(idx, 1);
      await env.CHANNELS.put('all', JSON.stringify(all));
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

function withActiveMembers(channel) {
  const now = Date.now();
  const ttl = 5 * 60 * 1000;
  const members = (channel.members || [])
    .filter((m) => m && m.toxid && now - (m.ts || 0) < ttl)
    .map((m) => m.toxid);
  return { ...channel, members };
}
