import type { Message } from "../../types";

export type DisplayItem =
  | { type: "date-separator"; date: string }
  | { type: "message-group"; senderId: string; messages: Message[] };

const FIVE_MINUTES_MS = 5 * 60 * 1000;

function getDateString(iso: string): string {
  return iso.slice(0, 10); // "YYYY-MM-DD"
}

export function groupMessages(messages: Message[]): DisplayItem[] {
  if (messages.length === 0) return [];

  const items: DisplayItem[] = [];
  let currentGroup: { senderId: string; messages: Message[] } | null = null;
  let currentDate: string | null = null;

  for (const msg of messages) {
    const msgDate = getDateString(msg.createdAt);
    const msgTime = new Date(msg.createdAt).getTime();

    // Date boundary check
    if (currentDate !== null && msgDate !== currentDate) {
      // Flush current group
      if (currentGroup) {
        items.push({ type: "message-group", ...currentGroup });
        currentGroup = null;
      }
      items.push({ type: "date-separator", date: msgDate });
    } else if (currentDate === null) {
      // First message - insert date separator
      items.push({ type: "date-separator", date: msgDate });
    }

    currentDate = msgDate;

    if (!currentGroup) {
      currentGroup = { senderId: msg.senderId, messages: [msg] };
      continue;
    }

    const lastMsg = currentGroup.messages[currentGroup.messages.length - 1];
    const lastTime = new Date(lastMsg.createdAt).getTime();
    const gap = msgTime - lastTime;

    if (msg.senderId !== currentGroup.senderId || gap >= FIVE_MINUTES_MS) {
      items.push({ type: "message-group", ...currentGroup });
      currentGroup = { senderId: msg.senderId, messages: [msg] };
    } else {
      currentGroup.messages.push(msg);
    }
  }

  if (currentGroup) {
    items.push({ type: "message-group", ...currentGroup });
  }

  return items;
}
