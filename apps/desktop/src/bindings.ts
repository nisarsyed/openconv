// This file is a placeholder stub for tauri-specta generated bindings.
// It will be overwritten when the Tauri app builds in debug mode.
// See: apps/desktop/src-tauri/src/lib.rs (specta export)

import { invoke } from "@tauri-apps/api/core";

export type AppHealth = {
  version: string;
  db_status: string;
};

export const commands = {
  async healthCheck(): Promise<AppHealth> {
    return await invoke<AppHealth>("health_check");
  },
};
