import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useAppStore } from "../store";
import type { ReadPositionInfo } from "../store/unreadSlice";

const SYNC_INTERVAL_MS = 30_000;

function logDevWarn(msg: string, err: unknown) {
  if (import.meta.env.DEV) console.warn(msg, err);
}

export function useReadPositionSync() {
  useEffect(() => {
    // Fetch and merge on mount
    invoke<ReadPositionInfo[]>("fetch_and_merge_read_positions")
      .then((positions) => {
        useAppStore.getState().initializeReadPositions(positions);
      })
      .catch((e) => logDevWarn("fetch_and_merge_read_positions failed:", e));

    // Periodic sync every 30s
    const interval = setInterval(() => {
      invoke("sync_read_positions").catch((e) =>
        logDevWarn("sync_read_positions failed:", e),
      );
    }, SYNC_INTERVAL_MS);

    // Sync on window close/blur
    const appWindow = getCurrentWindow();

    const unlistenClose = appWindow.onCloseRequested(async () => {
      await invoke("sync_read_positions").catch((e) =>
        logDevWarn("sync_read_positions on close failed:", e),
      );
    });

    const unlistenFocus = appWindow.onFocusChanged(({ payload: focused }) => {
      if (!focused) {
        invoke("sync_read_positions").catch((e) =>
          logDevWarn("sync_read_positions on blur failed:", e),
        );
      }
    });

    return () => {
      clearInterval(interval);
      unlistenClose.then((fn) => fn());
      unlistenFocus.then((fn) => fn());
    };
  }, []);
}
