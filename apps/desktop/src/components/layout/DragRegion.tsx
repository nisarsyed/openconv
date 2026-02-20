import { usePlatform } from "../../hooks/usePlatform";
import { GUILD_SIDEBAR_WIDTH } from "./constants";

export function DragRegion() {
  const os = usePlatform();

  if (os !== "macos") return null;

  // Only covers the guild sidebar area where traffic lights sit.
  // Other panels use data-tauri-drag-region on their headers.
  return (
    <div
      data-tauri-drag-region
      aria-hidden="true"
      className="absolute top-0 left-0 z-60"
      style={
        {
          width: GUILD_SIDEBAR_WIDTH + 4,
          height: "var(--titlebar-inset)",
          WebkitAppRegion: "drag",
        } as React.CSSProperties
      }
    />
  );
}
