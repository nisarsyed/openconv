import { platform } from "@tauri-apps/plugin-os";

export function usePlatform(): string | null {
  try {
    return platform();
  } catch {
    return null;
  }
}
