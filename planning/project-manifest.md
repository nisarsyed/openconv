<!-- SPLIT_MANIFEST
01-project-foundation
02-crypto-identity
03-auth-users
04-server-core
05-desktop-client
06-messaging-features
END_MANIFEST -->

# Project Manifest: OpenConv

A privacy-focused, lightweight desktop chat application (Discord alternative) built with Tauri + React/TS and a Rust/Axum server, featuring Signal protocol E2E encryption.

## Split Overview

### 01-project-foundation
**Purpose:** Establish the monorepo structure, shared code, scaffolding, and database schemas that all other splits depend on.

**Scope:**
- Cargo workspace with shared crates (types, models, error handling)
- Tauri 2.x desktop app scaffold
- Axum server scaffold with basic routing
- PostgreSQL schema design (server-side)
- SQLite schema design (client-side)
- npm workspace for React/TypeScript frontend
- Build tooling and dev workflow (hot reload, cross-compilation)
- CI basics

**Outputs:** Working monorepo where `cargo build` compiles server + client, `npm run dev` launches the Tauri app with hot reload, and both databases are initialized.

---

### 02-crypto-identity
**Purpose:** Implement the Signal protocol cryptography layer and keypair-based identity system.

**Scope:**
- libsignal-protocol integration (Rust bindings)
- Identity keypair generation and secure local storage
- X3DH key agreement protocol (initial key exchange)
- Double Ratchet algorithm (ongoing message encryption)
- Pre-key bundle generation and server upload
- Session management (crypto sessions between users)
- Encryption/decryption APIs consumed by messaging layer
- Key storage in client-side SQLite (encrypted)

**Outputs:** A Rust crate exposing encrypt/decrypt APIs, identity management, and key exchange protocols. No UI — consumed by other splits.

---

### 03-auth-users
**Purpose:** User registration, authentication, session management, and account recovery.

**Scope:**
- User registration flow (keypair generation → server registration with public key + email)
- Login flow (challenge-response using private key)
- Session/token management (JWT or similar)
- Email-based account recovery (key re-establishment)
- Server-side user storage (PostgreSQL: user profiles, public keys, pre-key bundles)
- Client-side auth UI (registration, login, recovery screens)
- User profile management (display name, avatar)

**Depends on:** 01 (schemas, scaffolds), 02 (keypair generation, pre-key bundles)

**Outputs:** Working auth flow — user can register, log in, and recover their account. Server stores user profiles and public keys.

---

### 04-server-core
**Purpose:** Server-side guild/channel management, permissions, WebSocket real-time layer, and storage APIs.

**Scope:**
- Server/guild CRUD (create, join, leave, manage)
- Channel CRUD within guilds
- Role-based permission system (owner, admin, member, custom roles)
- WebSocket server for real-time communication
- Message routing (receive encrypted blobs, store, fan out to recipients)
- Encrypted file/blob storage API (receive, store, serve encrypted files)
- REST API design for all server operations
- Rate limiting and basic abuse prevention

**Depends on:** 01 (scaffolds, schemas), 03 (auth/sessions — all endpoints are authenticated)

**Outputs:** Fully functional Axum server with REST + WebSocket APIs. Guilds, channels, roles, message storage, and file storage all working. Server handles encrypted blobs — it never decrypts content.

---

### 05-desktop-client
**Purpose:** Tauri desktop app shell with React UI, navigation, state management, and component library.

**Scope:**
- Tauri window management and system tray
- React app structure (routing, layout)
- UI component library (sidebar, server list, channel list, message view, user list, settings)
- State management (Zustand or similar — app state, server/channel state)
- Tauri IPC bridge (Rust ↔ JS communication patterns)
- Theming (dark mode primary, light mode support)
- Responsive layout design
- Accessibility basics

**Depends on:** 01 (Tauri scaffold, shared types)

**Note:** Can be developed in parallel with 02, 03, 04 since it initially uses mock data. Integration happens in 06.

**Outputs:** A polished desktop app shell with all UI screens, navigation, and mock data. Ready to wire up to real APIs.

---

### 06-messaging-features
**Purpose:** Wire everything together — real-time encrypted messaging, file sharing, offline support, notifications, and search.

**Scope:**
- WebSocket client in Tauri (connect, reconnect, heartbeat)
- E2E encrypted message send/receive flow (compose → encrypt → send → receive → decrypt → display)
- Channel messaging and direct messages (1:1)
- Message history loading and pagination
- Image sharing with inline previews (encrypt → upload → share link → download → decrypt → display)
- Small file sharing (<25MB, encrypted upload/download)
- SQLite local message cache (store decrypted messages locally)
- Offline mode (read cached messages, queue outgoing, sync on reconnect)
- Desktop notifications (Tauri notification API)
- Full-text message search (SQLite FTS5 on local cache)
- Unread message tracking and badge counts

**Depends on:** 02 (encryption), 03 (auth), 04 (server APIs), 05 (UI components)

**Outputs:** Fully functional chat application — users can message in channels, share files, work offline, receive notifications, and search messages.

---

## Dependency Graph

```
01-project-foundation
├── 02-crypto-identity
│   └── 03-auth-users (also depends on 01)
│       └── 04-server-core (also depends on 01)
│           └── 06-messaging-features (also depends on 02, 03, 05)
└── 05-desktop-client
        └── 06-messaging-features
```

## Execution Order

**Phase 1 (Sequential):**
1. `01-project-foundation` — Must come first. Everything depends on it.

**Phase 2 (Parallel):**
2. `02-crypto-identity` — Can start immediately after 01
3. `05-desktop-client` — Can start immediately after 01, in parallel with 02

**Phase 3 (Sequential after Phase 2):**
4. `03-auth-users` — Needs 01 + 02

**Phase 4 (Sequential after Phase 3):**
5. `04-server-core` — Needs 01 + 03

**Phase 5 (Final integration):**
6. `06-messaging-features` — Needs everything (02, 03, 04, 05)

## Cross-Cutting Concerns

- **Error handling pattern:** Define in 01, used everywhere
- **Shared types:** Rust crate in 01, shared between client and server
- **Database migrations:** Schema defined in 01, evolved in each split
- **E2E encryption boundary:** Server (04) only handles encrypted blobs; decryption happens in client (06) using crypto layer (02)

## /deep-plan Commands

```bash
/deep-plan @planning/01-project-foundation/spec.md
/deep-plan @planning/02-crypto-identity/spec.md
/deep-plan @planning/03-auth-users/spec.md
/deep-plan @planning/04-server-core/spec.md
/deep-plan @planning/05-desktop-client/spec.md
/deep-plan @planning/06-messaging-features/spec.md
```
