# Code Review Interview: Section 07 - React Frontend Scaffold

**Date:** 2026-02-15

## Interview Items

### Issue: TauRPC bindings vs raw invoke (HIGH - Review #1, #7, #10)
**Decision:** Use TauRPC bindings. Create a placeholder `src/bindings.ts` stub that mirrors the shape tauri-specta will generate. Update App.tsx to import from bindings. Update test mock to mock bindings module.

## Auto-Fixes

### Fix: Add console.error for failed IPC calls (Review #2)
Add `console.error` in the catch handler so failed health checks are visible in dev tools.

### Fix: Add error state test (Review #4)
Add a test that overrides the mock to reject and verifies the error UI renders.

### Fix: Move IPC mock from global setup to test file (Review #5)
Move the `vi.mock("@tauri-apps/api/core")` from `setup.ts` to `App.test.tsx` so other test files aren't affected. Update to mock the bindings module instead.

### Fix: Move afterEach inside describe block (Review #6)
Move the `afterEach` cleanup inside the `describe("App")` block.

## Let Go

- Review #3 (redundant dark mode): Both are needed — index.html prevents flash of unstyled content, useEffect ensures jsdom tests pass.
- Review #8 (extra tsconfig options): Beneficial for Vite projects, no harm.
- Review #9 (state leak cleanup): Handled by waitFor and vitest globals auto-cleanup.
