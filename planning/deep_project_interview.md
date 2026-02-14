# Deep Project Interview Transcript

## Project Overview
A privacy-focused, lightweight desktop chat application built as an alternative to Discord. Prioritizes performance, data ownership, and user privacy.

## Interview Q&A

### Round 1: Core Architecture

**Q: Tech stack for the desktop app?**
A: **Tauri + React/TypeScript** — Lightweight Rust-based desktop shell with React frontend. Strong privacy/performance story.

**Q: Backend/networking vision?**
A: **Centralized server** — User runs/connects to a centralized backend. Simpler architecture but can still be E2E encrypted.

**Q: Chat features in scope for V1?**
A: **Text + File sharing** — Text messaging with file/image sharing support.

**Q: What feels like the natural first thing to build?**
A: **Auth + identity first** — Start with user identity, encryption keys, and authentication.

### Round 2: Technical Details

**Q: Server language/framework?**
A: **Rust (Axum/Actix)** — Keep the whole stack in Rust. Tauri backend and server can share code.

**Q: Level of encryption?**
A: **E2E encryption (Signal protocol)** — Messages encrypted client-side. Server cannot read them. Gold standard.

**Q: Database preferences?**
A: **PostgreSQL + SQLite** — Postgres on server, SQLite on client for offline support and local data.

**Q: Repo structure?**
A: **Monorepo** — Single repo with packages/apps for client and server. Shared types/code.

### Round 3: Feature Scope

**Q: Chat structure?**
A: **Servers + Channels** — Discord-style: users create servers, servers have text channels. Role-based permissions.

**Q: Auth model?**
A: **Hybrid (keypair + recovery)** — Primary identity is keypair, but with email-based recovery option.

**Q: File sharing scope for V1?**
A: **Images + small files** — Share images inline and small files (<25MB). Server stores encrypted blobs.

**Q: Additional features/constraints?**
A: **Offline support** — App should work offline with local message cache and sync when back online.
A: **Notifications** — Push/desktop notifications for new messages.
A: **Message search** — Full-text search across message history.

## Consolidated Requirements

### Tech Stack
- **Desktop Client:** Tauri 2.x + React + TypeScript
- **Server:** Rust with Axum web framework
- **Client DB:** SQLite (via Tauri/rusqlite)
- **Server DB:** PostgreSQL
- **Monorepo:** Single repository with shared Rust crates

### Security & Privacy
- **E2E Encryption:** Signal protocol (libsignal)
- **Identity:** Keypair-based (public key IS identity)
- **Recovery:** Email-based account recovery
- **Data Ownership:** Server stores encrypted blobs only; cannot read message content

### Chat Model (Discord-like)
- **Servers/Guilds:** User-created communities
- **Channels:** Text channels within servers
- **Direct Messages:** 1:1 and group DMs
- **Roles & Permissions:** Role-based access control per server

### V1 Features
1. User registration and login (keypair generation, email recovery)
2. Server/guild creation and management
3. Channel creation and management within servers
4. Real-time text messaging (E2E encrypted)
5. Direct messages (1:1)
6. Image sharing with inline previews
7. Small file sharing (<25MB, encrypted)
8. Offline message cache with sync
9. Desktop notifications
10. Full-text message search

### Build Priority
Auth + identity system first, then server infrastructure, then client UI, then messaging features.
