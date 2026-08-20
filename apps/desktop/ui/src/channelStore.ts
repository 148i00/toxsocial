import { reactive } from "vue";

export interface ChannelMessage {
  /** Persisted row id (present once the message is in SQLite). */
  id?: number;
  conferenceNumber: number;
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
