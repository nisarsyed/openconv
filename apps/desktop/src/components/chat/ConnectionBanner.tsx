import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../../store";
import { Spinner } from "../ui/Spinner";

export function ConnectionBanner() {
  const connectionState = useAppStore((s) => s.connectionState);

  switch (connectionState.status) {
    case "Authenticated":
    case "Connected":
    case "Disconnected":
      return null;

    case "Connecting":
      return (
        <div
          data-testid="connection-banner"
          className="flex items-center justify-center gap-2 bg-[var(--bg-warning)] px-4 py-2 text-sm text-[var(--text-primary)]"
        >
          <Spinner size="sm" />
          <span>Connecting...</span>
        </div>
      );

    case "Reconnecting":
      return (
        <div
          data-testid="connection-banner"
          className="flex items-center justify-center gap-2 bg-[var(--bg-warning)] px-4 py-2 text-sm text-[var(--text-primary)]"
        >
          <Spinner size="sm" />
          <span>Reconnecting... (attempt {connectionState.attempt + 1})</span>
        </div>
      );

    case "Failed":
      return (
        <div
          data-testid="connection-banner"
          className="flex items-center justify-center gap-2 bg-[var(--bg-danger)] px-4 py-2 text-sm text-[var(--text-primary)]"
        >
          <span>Connection lost. Please check your internet connection.</span>
          <button
            type="button"
            className="rounded bg-[var(--bg-accent)] px-3 py-1 text-xs font-medium hover:opacity-90"
            onClick={() => invoke("ws_connect").catch(console.error)}
          >
            Retry
          </button>
        </div>
      );
  }
}
