# 06 - Messaging & Features

## Overview

The integration split — wire together the crypto layer, server APIs, and desktop client UI into a working encrypted chat application. Implements real-time messaging, file sharing, offline support, desktop notifications, and message search.

This is the final split where everything comes together.

## Original Requirements

See `planning/requirements.md` for the high-level project description.
See `planning/deep_project_interview.md` for full interview context.

## Key Decisions (from interview)

- **E2E encryption:** All messages encrypted client-side using Signal protocol (from 02)
- **File sharing:** Images + small files (<25MB), encrypted
- **Offline support:** SQLite local cache, queue outgoing, sync on reconnect
- **Notifications:** Desktop notifications for new messages
- **Search:** Full-text search on local message cache

## Scope

### WebSocket Client
- WebSocket connection management in Tauri:
  - Connect to server's `/ws` endpoint with auth token
  - Automatic reconnection with exponential backoff
  - Heartbeat/ping-pong keep-alive
  - Connection state tracking (connected, connecting, disconnected)
- Event handling:
  - Receive messages, presence updates, typing indicators
  - Send messages, typing notifications
  - Channel subscription management

### E2E Encrypted Messaging Flow
- **Sending a message:**
  1. User types message in input
  2. Client looks up/creates Signal session with each recipient
  3. Client encrypts message using 02-crypto-identity crate
  4. Client sends encrypted blob via WebSocket to server
  5. Server stores and fans out encrypted blob
- **Receiving a message:**
  1. Server delivers encrypted blob via WebSocket
  2. Client decrypts using Signal session
  3. Client stores decrypted message in local SQLite cache
  4. Client renders message in the UI
- **Key exchange (first message to a user):**
  1. Client fetches recipient's pre-key bundle from server
  2. Client performs X3DH key agreement
  3. Client initializes Double Ratchet session
  4. Client encrypts and sends message

### Channel Messaging
- Real-time message display as messages arrive
- Message history loading (paginated, from server)
- Decryption of historical messages (requires session state)
- Message sending in channels and DMs
- Message editing (re-encrypt, send update)
- Message deletion (send delete event)
- Typing indicators (send/receive)

### Direct Messages
- Start a DM conversation (key exchange + first message)
- DM list in sidebar
- Same message flow as channels but 1:1

### File Sharing
- **Sending a file:**
  1. User selects file (image or <25MB file)
  2. Client generates random symmetric key
  3. Client encrypts file with symmetric key
  4. Client uploads encrypted blob to server's file endpoint
  5. Client encrypts the symmetric key per-recipient (using Signal session)
  6. Client sends a message containing: file metadata + encrypted symmetric key + server file reference
- **Receiving a file:**
  1. Client receives message with file reference
  2. Client decrypts the symmetric key from the message
  3. Client downloads encrypted blob from server
  4. Client decrypts blob with symmetric key
  5. For images: render inline preview
  6. For files: show download button, save to user's filesystem via Tauri
- Image thumbnail generation (client-side, after decryption)
- File type validation and size checks

### Offline Support
- **Local message cache (SQLite):**
  - Store decrypted messages in client-side SQLite
  - Cache guild/channel metadata
  - Cache user profiles and avatars
- **Offline reading:**
  - App loads cached data when offline
  - Show connection status indicator
  - All cached messages are searchable
- **Message queue:**
  - Queue outgoing messages when offline
  - Send queued messages on reconnect
  - Show "pending" state on queued messages
- **Sync protocol:**
  - On reconnect, fetch messages since last received timestamp
  - Merge server messages with local cache
  - Handle conflicts (e.g., messages deleted while offline)

### Desktop Notifications
- Tauri notification API integration
- Notify on new messages (when window is not focused or channel is not visible)
- Notification content: sender name + message preview (decrypted client-side)
- Click notification → focus window and navigate to channel
- Notification settings (mute per channel, mute per guild, global DND)
- Respect OS notification settings

### Message Search
- SQLite FTS5 (Full-Text Search) on local message cache
- Search UI:
  - Search input in top bar
  - Search results with message previews and channel context
  - Click result → navigate to message in channel
- Search scope: all messages, current guild, current channel
- Search indexing: index messages as they are decrypted and cached

### Unread Tracking
- Track last read position per channel (client-side + synced to server)
- Unread message count badges on channels and guilds
- Mark as read when channel is viewed
- Mention detection (future: @user mentions)

## Outputs

1. Fully functional encrypted chat — send and receive messages in real-time
2. File/image sharing working end-to-end (encrypted)
3. Offline mode — read cached messages, queue outgoing
4. Desktop notifications on new messages
5. Full-text message search across local cache
6. Unread tracking with badges

## Dependencies

- **Depends on:** 02 (encryption/decryption APIs), 03 (auth tokens, user identity), 04 (server APIs, WebSocket), 05 (UI components, state management)
- **Depended on by:** Nothing — this is the final integration split

## Technical Notes

- Signal protocol group messaging (Sender Keys) is complex — for V1, consider pairwise encryption for channel messages (each message encrypted per-recipient). This is less efficient but much simpler. Optimize with Sender Keys in V2.
- WebSocket reconnection must re-establish subscriptions and sync missed messages
- SQLite FTS5 provides good full-text search performance and is built into SQLite
- File encryption uses symmetric keys (AES-256-GCM recommended), not Signal sessions — Signal sessions encrypt the key, not the file
- Consider message batching for offline sync (don't fetch one-by-one)
- Notification privacy: only show decrypted content in notifications if user has enabled it; otherwise show "New message from [sender]"
