# 04 - Server Core

## Overview

Build the Axum server's core functionality: guild/channel management, role-based permissions, WebSocket real-time layer, and encrypted message/file storage. The server operates as a blind relay — it routes and stores encrypted blobs without ever accessing plaintext content.

## Original Requirements

See `planning/requirements.md` for the high-level project description.
See `planning/deep_project_interview.md` for full interview context.

## Key Decisions (from interview)

- **Chat model:** Discord-style servers (guilds) with channels
- **Permissions:** Role-based access control per guild
- **Server framework:** Axum
- **Storage:** PostgreSQL for metadata, encrypted blobs for message/file content
- **Privacy principle:** Server NEVER decrypts — it only stores and relays encrypted data

## Scope

### Guild (Server) Management
- REST endpoints for guild CRUD:
  - Create guild (name, icon)
  - Update guild settings
  - Delete guild (owner only)
  - List user's guilds
- Invite system (invite codes/links)
- Join/leave guild
- Guild icon upload and storage

### Channel Management
- REST endpoints for channel CRUD within a guild:
  - Create channel (name, type: text)
  - Update channel (name, topic, position)
  - Delete channel
  - Reorder channels
- Channel permissions (who can read, who can write)
- Default channel on guild creation

### Role-Based Permissions
- Role CRUD within a guild
- Built-in roles: owner, admin, member
- Custom roles with granular permissions:
  - Manage guild, manage channels, manage roles
  - Send messages, read messages, attach files
  - Kick/ban members, manage invites
- Role hierarchy (higher roles override lower)
- Permission resolution: user permissions = union of all their roles' permissions
- Permission checking middleware

### WebSocket Real-Time Layer
- WebSocket upgrade endpoint (`/ws`)
- Connection authentication (token-based)
- Connection lifecycle (connect, heartbeat, disconnect, reconnect)
- Event routing:
  - Subscribe to channels (join channel → receive messages)
  - Unsubscribe from channels
  - Presence updates (online/offline/typing)
- Message fan-out (server receives encrypted message, delivers to all channel subscribers)
- Connection state tracking (which users are in which channels)

### Message Storage & Routing
- Receive encrypted message blobs via WebSocket
- Store in PostgreSQL (messages table: id, channel_id, sender_id, encrypted_content, timestamp)
- Fan out to connected recipients via WebSocket
- Message history endpoint (paginated, by channel)
- Message deletion (soft delete — mark as deleted, remove content)
- Message editing (replace encrypted_content, mark as edited)

### File Storage
- File upload endpoint (multipart, encrypted blob)
- File metadata storage (size, mime type, uploader, timestamp)
- File download endpoint (serve encrypted blob)
- Storage backend (local filesystem for V1, abstractable to S3 later)
- File size validation (<25MB)
- Cleanup for orphaned files

### Direct Messages
- DM channel creation (1:1)
- DM channel listing
- DMs share the same message infrastructure as guild channels

### API Design
- RESTful endpoints for all CRUD operations
- WebSocket for real-time events
- Consistent error response format
- API versioning strategy (e.g., `/api/v1/...`)
- OpenAPI/swagger documentation (optional, nice-to-have)

### Operational Concerns
- Rate limiting per user (message sending, API calls)
- Request logging and tracing
- Database connection pooling
- Basic abuse prevention (spam detection placeholder)

## Outputs

1. Fully functional Axum server with REST + WebSocket APIs
2. Guild/channel/role CRUD working
3. Permission system enforced on all endpoints
4. Real-time message routing via WebSocket
5. Encrypted message and file storage
6. DM channels functional

## Dependencies

- **Depends on:** 01 (server scaffold, database schemas, shared types), 03 (auth middleware — all endpoints require authentication)
- **Depended on by:** 06 (client connects to these APIs for messaging)

## Technical Notes

- The server is a "zero-knowledge" relay — design all storage to handle opaque encrypted blobs
- WebSocket protocol should be well-defined (JSON message types with discriminator field)
- Consider using tokio broadcast channels for in-process message fan-out
- Guild member counts and channel read positions are server-side metadata (not encrypted)
- File storage abstraction: trait-based so local FS can be swapped for S3/MinIO later
