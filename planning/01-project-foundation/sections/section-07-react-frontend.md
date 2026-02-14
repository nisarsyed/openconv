No sections have been generated yet, so I have the full context from the plan and TDD files. Now I have everything needed to generate the section. Here is the output:

# Section 07: React Frontend Scaffold

## Overview

This section sets up the React 19 + TypeScript frontend for the Tauri desktop application. It creates the Vite build configuration, Tailwind CSS v4 styling with dark mode, the application shell (`App.tsx`), and integrates with the TauRPC-generated bindings from the Tauri backend. The frontend lives at `apps/desktop/` and serves as the UI layer rendered inside the Tauri webview.

## Dependencies

- **section-01-monorepo-setup**: The npm workspace root `package.json` at the repo root must exist with `"workspaces": ["apps/desktop"]`. The `apps/desktop/` directory must exist.
- **section-05-tauri-scaffold**: The Tauri backend must be scaffolded at `apps/desktop/src-tauri/`. TauRPC (or fallback `ts-rs`) must be configured to generate TypeScript bindings into `apps/desktop/src/bindings.ts`. The `tauri.conf.json` must point the dev server at `http://localhost:1420` and the build output at `../dist`.

## Tests First

All frontend tests use Vitest with React Testing Library. The test configuration itself is part of this section; the actual test file is a smoke test verifying the application shell renders correctly.

### Test File: `/Users/nisar/personal/projects/openconv/apps/desktop/src/__tests__/App.test.tsx`

```typescript
// Test: App component renders without crashing (smoke test)
// Test: App component renders the OpenConv title text
// Test: App component mounts in dark mode (has 'dark' class on root)
// Test: App component displays a status indicator element
```

The smoke test should:

1. Render the `<App />` component using React Testing Library's `render()`.
2. Assert that the component mounts without throwing.
3. Assert that the text "OpenConv" is present in the rendered output (using `screen.getByText` or similar).
4. Assert that the document's root element (or a wrapper div) has the `dark` CSS class applied.
5. Assert that a status indicator element exists (e.g., an element with `data-testid="status-indicator"` or role-based query).

Because the health check IPC call goes through TauRPC bindings that are unavailable in the test environment, the test must mock the TauRPC client. The mock should return a resolved promise with a mock health response so the component can render its status indicator.

### Test Configuration: `/Users/nisar/personal/projects/openconv/apps/desktop/vitest.config.ts`

```typescript
/// <reference types="vitest" />
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/__tests__/setup.ts"],
  },
});
```

### Test Setup: `/Users/nisar/personal/projects/openconv/apps/desktop/src/__tests__/setup.ts`

This file imports `@testing-library/jest-dom/vitest` (or the equivalent matchers) to enable DOM-specific assertions like `toBeInTheDocument()`. It should also mock the Tauri IPC layer (`@tauri-apps/api`) so that tests do not attempt real IPC calls.

## Implementation Details

### 7.1 Vite Configuration

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/vite.config.ts`

Create a Vite config with the React plugin. Key settings:

- **Server port:** 1420 (matches `tauri.conf.json` dev server URL)
- **Strict port:** `true` (fail if port 1420 is taken rather than picking another)
- **File watcher ignore:** Add `src-tauri/` to the watcher ignore list so that Rust recompilation does not trigger Vite hot reload
- **Plugin:** `@vitejs/plugin-react`

```typescript
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 1420,
    strictPort: true,
  },
  // Prevent Vite from watching Rust files
  server: {
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
```

Note: Merge both `server` keys into a single `server` object in the actual implementation.

### 7.2 TypeScript Configuration

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/tsconfig.json`

TypeScript in strict mode. Key compiler options:

- `"strict": true`
- `"target": "ES2020"` or later
- `"module": "ESNext"`
- `"moduleResolution": "bundler"`
- `"jsx": "react-jsx"`
- `"esModuleInterop": true`
- `"skipLibCheck": true`
- `"forceConsistentCasingInFileNames": true`
- Include `"src"` directory
- Exclude `"node_modules"`, `"dist"`, `"src-tauri"`

### 7.3 Tailwind CSS v4 Setup

Tailwind CSS v4 uses a new CSS-first configuration approach rather than the `tailwind.config.ts` file of v3. The setup involves:

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src/index.css`

The base CSS file uses Tailwind v4's `@import` syntax:

```css
@import "tailwindcss";
```

For dark mode as the default, the `dark` class is applied to the root `<html>` element in `index.html` (or programmatically in `main.tsx`). Tailwind v4 supports the `dark:` variant via the `class` strategy by default when using `@import "tailwindcss"`.

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/postcss.config.js`

PostCSS config with the Tailwind CSS plugin:

```javascript
export default {
  plugins: {
    "@tailwindcss/postcss": {},
  },
};
```

### 7.4 HTML Entry Point

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/index.html`

Standard Vite HTML entry point. The `<html>` element must have `class="dark"` to enable dark mode by default:

```html
<!doctype html>
<html lang="en" class="dark">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>OpenConv</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

### 7.5 React Entry Point

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src/main.tsx`

The React root with StrictMode:

```typescript
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

### 7.6 Application Shell

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/src/App.tsx`

The minimal app component that:

1. Imports the TauRPC-generated bindings from `./bindings` (generated by the Tauri backend build step into `src/bindings.ts`)
2. Uses a `useEffect` hook on mount to call the `health_check` IPC command
3. Stores the health check result in component state
4. Renders a placeholder layout with:
   - The "OpenConv" title
   - A status indicator showing whether IPC is connected (green dot) or not yet connected (gray dot)
   - Dark-themed styling via Tailwind utility classes
5. The root wrapper div should apply dark theme background and text colors (e.g., `bg-gray-900 text-gray-100 min-h-screen`)

The component should gracefully handle the case where the IPC call fails (e.g., during testing or if the backend is not ready). Display an error state rather than crashing.

The status indicator element should have `data-testid="status-indicator"` for test accessibility.

Stub structure:

```typescript
import { useState, useEffect } from "react";

/**
 * Root application component.
 * Renders the OpenConv shell with dark theme and verifies
 * IPC connectivity via a health check command on mount.
 */
function App() {
  // State for health check result (null = loading, object = success, error string = failure)
  // useEffect to call health_check IPC command on mount
  // Render: title, status indicator, placeholder content
}

export default App;
```

### 7.7 Package Dependencies

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/package.json`

This is the desktop app's `package.json` (within the npm workspace). It contains:

**Production dependencies:**
- `react` (^19)
- `react-dom` (^19)
- `react-router` (^7) -- for future routing, installed now
- `@tauri-apps/api` (^2) -- Tauri frontend API
- `@tauri-apps/plugin-shell` (^2) -- if needed for shell commands

**Dev dependencies:**
- `typescript` (^5.5)
- `@types/react` (^19)
- `@types/react-dom` (^19)
- `vite` (^6)
- `@vitejs/plugin-react` (^4)
- `tailwindcss` (^4)
- `@tailwindcss/postcss` -- PostCSS plugin for Tailwind v4
- `postcss` (^8)
- `autoprefixer` (^10)
- `eslint` (^9)
- `prettier` (^3)
- `vitest` (^3)
- `@testing-library/react` (^16)
- `@testing-library/jest-dom` (^6)
- `@testing-library/user-event` (^14)
- `jsdom` (^25) -- for Vitest's jsdom environment

**Scripts:**

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "test": "vitest run",
    "test:watch": "vitest",
    "lint": "eslint src/",
    "fmt": "prettier --write src/",
    "fmt:check": "prettier --check src/"
  }
}
```

The `name` field should be `"openconv-desktop"`. Set `"private": true`. Set `"type": "module"` for ESM.

### 7.8 ESLint Configuration

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/eslint.config.js`

Use ESLint v9 flat config format. Include the TypeScript and React plugins. A minimal configuration is sufficient for the foundation -- more rules can be added later.

### 7.9 Prettier Configuration

**File:** `/Users/nisar/personal/projects/openconv/apps/desktop/.prettierrc`

Minimal Prettier config:

```json
{
  "semi": true,
  "singleQuote": false,
  "tabWidth": 2,
  "trailingComma": "all"
}
```

## File Listing Summary

All files to be created by this section:

| File | Purpose |
|------|---------|
| `/Users/nisar/personal/projects/openconv/apps/desktop/package.json` | npm package manifest with dependencies and scripts |
| `/Users/nisar/personal/projects/openconv/apps/desktop/vite.config.ts` | Vite build configuration (port 1420, React plugin) |
| `/Users/nisar/personal/projects/openconv/apps/desktop/tsconfig.json` | TypeScript strict mode configuration |
| `/Users/nisar/personal/projects/openconv/apps/desktop/postcss.config.js` | PostCSS config with Tailwind v4 plugin |
| `/Users/nisar/personal/projects/openconv/apps/desktop/index.html` | HTML entry point with dark class on root |
| `/Users/nisar/personal/projects/openconv/apps/desktop/src/main.tsx` | React root with StrictMode |
| `/Users/nisar/personal/projects/openconv/apps/desktop/src/App.tsx` | Application shell with health check IPC |
| `/Users/nisar/personal/projects/openconv/apps/desktop/src/index.css` | Base CSS with Tailwind v4 import |
| `/Users/nisar/personal/projects/openconv/apps/desktop/vitest.config.ts` | Vitest test configuration |
| `/Users/nisar/personal/projects/openconv/apps/desktop/src/__tests__/setup.ts` | Test setup (DOM matchers, Tauri mock) |
| `/Users/nisar/personal/projects/openconv/apps/desktop/src/__tests__/App.test.tsx` | App component smoke test |
| `/Users/nisar/personal/projects/openconv/apps/desktop/eslint.config.js` | ESLint v9 flat config |
| `/Users/nisar/personal/projects/openconv/apps/desktop/.prettierrc` | Prettier formatting config |

## Verification Checklist

After implementation, verify:

1. `cd apps/desktop && npm install` succeeds with no peer dependency errors
2. `cd apps/desktop && npm run build` produces output in `dist/`
3. `cd apps/desktop && npm test` runs Vitest and the smoke test passes
4. `cd apps/desktop && npm run lint` passes with no errors
5. `cd apps/desktop && npm run fmt:check` passes
6. The Vite dev server starts on port 1420 when running `npm run dev`
7. When launched inside Tauri (`just dev`), the dark-themed placeholder UI renders in the webview window
8. The health check IPC call succeeds and the status indicator shows a connected state