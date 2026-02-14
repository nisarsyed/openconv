# Code Review Interview: Section 08 - Dev Tooling

**Date:** 2026-02-15

## Auto-Fixes

### Fix: Change SERVER_HOST from 0.0.0.0 to 127.0.0.1 in .env.example (Review #1)
The .env.example was created in section-01 with SERVER_HOST=0.0.0.0, which contradicts the plan's safety requirement for localhost-only binding. Changed to 127.0.0.1 to match config.toml.

### Fix: Add default recipe to justfile (Review #2)
Added `default` recipe that runs `just --list` for better ergonomics when running bare `just`.

## Let Go

- Review #3 (restart policy): Not required by plan, local dev context is fine.
