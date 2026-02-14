# 03 - Authentication & User Management

## Overview

Implement user registration, authentication, session management, and account recovery for OpenConv. Auth is keypair-based (public key is identity) with email-based recovery as a fallback.

This split bridges the crypto layer (02) with the server (04) and client UI (05).

## Original Requirements

See `planning/requirements.md` for the high-level project description.
See `planning/deep_project_interview.md` for full interview context.

## Key Decisions (from interview)

- **Auth model:** Hybrid — keypair-based identity + email recovery
- **Build priority:** Auth + identity system first (user's stated preference)
- **User identity:** Public key IS the user's identity
- **Recovery:** Email-based account recovery for lost keys

## Scope

### User Registration
- Client-side: generate identity keypair (via 02-crypto-identity crate)
- Client-side: generate initial pre-key bundle
- Server endpoint: `POST /api/auth/register`
  - Accepts: public key, pre-key bundle, email, display name
  - Creates user record, stores public key and pre-keys
  - Returns: user ID, auth token
- Registration UI screen in the Tauri client

### Authentication (Login)
- Challenge-response protocol:
  1. Client sends public key to server
  2. Server generates random challenge, returns it
  3. Client signs challenge with private key
  4. Server verifies signature against stored public key
  5. Server issues session token (JWT or similar)
- Login UI screen
- Token refresh mechanism
- Logout (token invalidation)

### Session Management
- JWT or similar token-based sessions
- Token storage on client (secure, not localStorage)
- Automatic token refresh
- Multi-device session tracking (design for future, single device V1)
- Session expiration and renewal

### Account Recovery
- Email verification during registration (confirmation link/code)
- Recovery flow:
  1. User triggers recovery via email
  2. Server sends recovery link
  3. User generates new keypair on device
  4. User proves email ownership
  5. Server updates public key, invalidates old sessions
  6. User re-establishes crypto sessions with contacts
- Recovery UI screens

### User Profiles
- Display name, avatar upload
- Profile update endpoints
- User search/lookup by display name or public key fingerprint
- Online/offline status tracking

### Server-Side (Axum endpoints)
- All auth-related REST endpoints
- User CRUD operations
- Pre-key bundle storage and retrieval endpoints
- Email sending integration (SMTP or third-party service)
- Rate limiting on auth endpoints

### Client-Side (Tauri + React)
- Auth state management (logged in/out, token storage)
- Registration form with keypair generation
- Login screen
- Recovery flow screens
- Profile editing screen
- Auth guards (redirect to login if not authenticated)

## Outputs

1. Working registration → login → session flow
2. Email-based recovery functional
3. Server stores user profiles and public keys
4. Client manages auth state and tokens
5. Pre-key bundles uploaded to server during registration

## Dependencies

- **Depends on:** 01 (scaffolds, schemas, shared types), 02 (keypair generation, pre-key bundle creation)
- **Depended on by:** 04 (auth middleware, user context), 06 (user identity in messaging)

## Technical Notes

- The challenge-response login avoids sending passwords — the server never has the private key
- Email service can be abstracted behind a trait for testing (mock) and production (real SMTP)
- Pre-key bundle replenishment: server should notify client when one-time pre-keys are running low
- Consider rate limiting and brute-force protection on the challenge-response flow
