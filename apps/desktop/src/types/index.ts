// Wire Types (match Rust API exactly)

export interface WireMessage {
  id: string;
  channelId: string;
  senderId: string;
  encryptedContent: string;
  nonce: string;
  createdAt: string;
}

export interface WireUserProfile {
  id: string;
  displayName: string;
  avatarUrl: string | null;
}

// Client Types (decrypted, UI-facing)

export interface FileAttachment {
  id: string;
  fileName: string;
  fileSize: number;
  mimeType: string;
  url: string;
  thumbnailUrl: string | null;
}

export interface Message {
  id: string;
  channelId: string;
  senderId: string;
  content: string;
  encryptedContent: string;
  nonce: string;
  createdAt: string;
  editedAt: string | null;
  attachments: FileAttachment[];
}

export interface User {
  id: string;
  displayName: string;
  avatarUrl: string | null;
  email: string;
}

export interface Guild {
  id: string;
  name: string;
  ownerId: string;
  iconUrl: string | null;
}

export interface Channel {
  id: string;
  guildId: string;
  name: string;
  channelType: "text" | "voice";
  position: number;
  category: string | null;
}

export interface Member {
  userId: string;
  guildId: string;
  nickname: string | null;
  roles: string[];
  joinedAt: string;
}

export interface Role {
  id: string;
  guildId: string;
  name: string;
  color: string;
  position: number;
}

export interface Notification {
  id: string;
  type: "error" | "success" | "info";
  message: string;
  dismissAfterMs: number | null;
}

export type PresenceStatus = "online" | "idle" | "dnd" | "offline";

export type ModalType =
  | { type: "createGuild" }
  | { type: "createChannel"; guildId: string }
  | { type: "invite"; guildId: string }
  | { type: "imageViewer"; imageUrl: string; allImages?: string[] }
  | { type: "confirm"; title: string; message: string; onConfirm: () => void };
