# 05 - Desktop Client UI

## Overview

Build the Tauri desktop application shell with React UI — all screens, navigation, components, state management, and theming. This split focuses on the UI layer and client architecture. It uses mock data initially; real API integration happens in 06.

## Original Requirements

See `planning/requirements.md` for the high-level project description.
See `planning/deep_project_interview.md` for full interview context.

## Key Decisions (from interview)

- **Framework:** Tauri 2.x + React + TypeScript
- **Chat model:** Discord-style layout (guild sidebar, channel list, message view)
- **State management:** To be decided (Zustand recommended for simplicity)
- **Theme:** Dark mode primary

## Scope

### Tauri Configuration
- Window management (main window size, min size, title bar customization)
- System tray with context menu (show/hide, quit, status)
- Tauri IPC command patterns (invoke Rust from JS, events from Rust to JS)
- Auto-updater configuration (placeholder for future)
- Platform-specific adjustments (macOS traffic lights, Windows title bar)

### Application Layout
- Discord-inspired three-panel layout:
  - **Left sidebar:** Guild/server list (icons), channel list for selected guild
  - **Center panel:** Message view for selected channel, message input area
  - **Right sidebar:** Member list for current channel/guild (collapsible)
- Top bar: Channel name, search, settings
- Responsive behavior: right sidebar collapses on narrow windows

### Navigation & Routing
- Client-side routing (React Router or TanStack Router)
- Routes:
  - `/login`, `/register`, `/recover` (auth screens, from 03)
  - `/app` (main chat view)
  - `/app/guild/:guildId/channel/:channelId` (specific channel)
  - `/app/dm/:dmId` (direct message)
  - `/app/settings` (user settings)
  - `/app/guild/:guildId/settings` (guild settings)
- Route guards (redirect to login if unauthenticated)

### UI Components
- **Guild sidebar:**
  - Guild icon list with selection state
  - Add guild button, guild creation modal
  - DM button (switch to DM list)
  - Unread indicators / badge counts
- **Channel list:**
  - Channel categories (collapsible groups)
  - Channel items with type icon and unread state
  - Create channel modal
- **Message view:**
  - Message list with virtualized scrolling (handle large histories)
  - Message bubbles: avatar, username, timestamp, content, file attachments
  - Message grouping (consecutive messages from same user)
  - Date separators
  - "New messages" indicator on scroll
  - Loading states (fetching history, sending message)
- **Message input:**
  - Text input with multiline support
  - File attachment button (image + file picker)
  - File preview before sending
  - Typing indicator display
  - Send button / Enter to send
- **Member list:**
  - User list with online/offline status
  - Role badges
  - User profile popover on click
- **Settings screens:**
  - User profile editing (display name, avatar)
  - Account settings
  - Guild settings (name, icon, roles, channels)
  - Appearance settings (theme toggle)
- **Modals and overlays:**
  - Guild creation, channel creation
  - Invite link generation/sharing
  - User profile popover
  - Image viewer (click to expand inline images)
  - File download confirmation

### State Management
- Global state store (Zustand recommended):
  - Auth state (user, token, login status)
  - Guild/channel data (list of guilds, channels per guild)
  - Messages per channel (with pagination state)
  - UI state (selected guild, selected channel, sidebar visibility)
  - Presence data (online users)
- State persistence for offline support (serialize to SQLite via Tauri IPC)

### Theming
- Dark mode as default
- Light mode support
- CSS custom properties or CSS-in-JS theming
- Consistent color palette, typography, spacing

### Tauri IPC Bridge
- Define Rust IPC commands for:
  - SQLite operations (read/write local cache)
  - Crypto operations (encrypt/decrypt via 02 crate)
  - File system operations (save downloaded files)
  - Notification triggers
  - System info (for about screen)
- TypeScript types for all IPC commands (generated or hand-written)

### Accessibility
- Keyboard navigation (Tab, arrow keys in lists)
- Focus management (modal focus traps)
- Semantic HTML
- Screen reader labels for icons and status indicators

## Outputs

1. Polished Tauri desktop app with all UI screens
2. Navigation between all views
3. Component library with mock data
4. State management wired up
5. Dark/light theme working
6. Ready to connect to real APIs in split 06

## Dependencies

- **Depends on:** 01 (Tauri scaffold, shared types)
- **Depended on by:** 06 (UI components used for real messaging)

**Parallel note:** This split can be developed simultaneously with 02, 03, and 04 since it uses mock data. The integration with real backends happens in 06.

## Technical Notes

- Use virtualized lists (react-window or TanStack Virtual) for message views — channels may have thousands of messages
- Tauri 2.x has a different plugin/permission model than 1.x — plan accordingly
- Consider react-query or TanStack Query for server state management (caching, refetching) when APIs are wired up in 06
- Image previews: generate thumbnails client-side for fast display
- The IPC bridge is the most critical architecture decision — well-typed commands prevent runtime errors
