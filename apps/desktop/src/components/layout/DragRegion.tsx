import { usePlatform } from "../../hooks/usePlatform";

export function DragRegion() {
  const os = usePlatform();

  if (os !== "macos") return null;

  return (
    <div
      data-tauri-drag-region
      aria-hidden="true"
      className="h-7 w-full shrink-0"
      style={{ WebkitAppRegion: "drag" } as React.CSSProperties}
    />
  );
}
