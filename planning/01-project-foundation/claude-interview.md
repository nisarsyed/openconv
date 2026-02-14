# Interview Transcript: 01-Project Foundation

## Q1: ID Strategy for typed IDs (UserId, GuildId, etc.)
**Answer:** UUIDs (v7 recommended) — Time-sortable, no coordination needed, 128-bit. Good for distributed systems.

## Q2: Client SQLite approach
**Answer:** rusqlite via Tauri commands — Full Rust control over queries, better for E2E encryption (crypto ops in Rust). More boilerplate but more control.

## Q3: Server configuration management
**Answer:** Up to Claude — I'll use environment variables via dotenvy + a TOML config file with env overrides. This gives structured config for many settings while keeping 12-factor compatibility.

## Q4: Build/dev task runner
**Answer:** just (justfile) — Modern, Rust-based, better UX, purpose-built for project commands.

## Q5: Database schema scope in foundation split
**Answer:** All tables now — Define the complete schema upfront in this foundation split. Other splits add columns/indexes as needed.

## Q6: React frontend styling approach
**Answer:** Tailwind CSS — Utility-first, fast iteration, widely adopted. Pairs well with Vite.

## Q7: Type-safe IPC between Tauri Rust and React
**Answer:** TauRPC (auto-generated) — Uses Specta for automatic TS type generation from Rust. Less boilerplate, stays in sync automatically.
