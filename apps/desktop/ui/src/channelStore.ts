import { reactive } from "vue";

export interface ChannelMessage {
  /** Persisted row id (present once the message is in SQLite). */
  id?: number;
  conferenceNumber: number;
  /** Stable channel id. The display filter prefers it over the conference
   * number, because toxcore reuses numbers after a channel is deleted. */
  channelId?: string;
  channelName: string;
  peer: string;
  text: string;
  ts?: number;
}

export const MAX_CHANNEL_MESSAGES = 300;

export const channelMessages = reactive<ChannelMessage[]>([]);

export function pushChannelMessage(message: ChannelMessage) {
  channelMessages.push(message);
  if (channelMessages.length > MAX_CHANNEL_MESSAGES) {
    channelMessages.splice(0, channelMessages.length - MAX_CHANNEL_MESSAGES);
  }
}

/**
 * Drop all in-memory messages of a deleted channel. Required because toxcore
 * reuses conference numbers: without this, a new channel on the same number
 * would display the old channel's buffered messages (including queued ones).
 */
export function clearChannelMessages(channelId: string, conferenceNumber: number) {
  const before = channelMessages.length;
  for (let i = channelMessages.length - 1; i >= 0; i--) {
    const m = channelMessages[i];
    if ((m.channelId && m.channelId === channelId) || (!m.channelId && m.conferenceNumber === conferenceNumber)) {
      channelMessages.splice(i, 1);
    }
  }
  return before - channelMessages.length;
}

/**
 * Merge persisted history into the in-memory buffer without duplicating
 * messages that are already there (matched by row id, or by
 * (peer, text, ts) as a fallback for messages without an id).
 */
export function mergeChannelHistory(history: ChannelMessage[]) {
  let added = 0;
  for (const m of history) {
    const dup = channelMessages.some(
      (x) =>
        (m.id !== undefined && x.id !== undefined && x.id === m.id) ||
        (x.peer === m.peer && x.text === m.text && x.ts !== undefined && x.ts === m.ts),
    );
    if (dup) continue;
    channelMessages.push(m);
    added++;
  }
  if (channelMessages.length > MAX_CHANNEL_MESSAGES) {
    channelMessages.splice(0, channelMessages.length - MAX_CHANNEL_MESSAGES);
  }
  return added;
}
