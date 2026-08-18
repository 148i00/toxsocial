import { reactive } from "vue";

export interface ChannelMessage {
  conferenceNumber: number;
  channelName: string;
  peer: string;
  text: string;
}

export const MAX_CHANNEL_MESSAGES = 300;

export const channelMessages = reactive<ChannelMessage[]>([]);

export function pushChannelMessage(message: ChannelMessage) {
  channelMessages.push(message);
  if (channelMessages.length > MAX_CHANNEL_MESSAGES) {
    channelMessages.splice(0, channelMessages.length - MAX_CHANNEL_MESSAGES);
  }
}
