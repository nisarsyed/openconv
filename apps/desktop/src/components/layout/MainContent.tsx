import { Outlet } from "react-router";
import { ChannelHeader } from "../chat/ChannelHeader";

export function MainContent() {
  return (
    <main data-testid="main-content" className="flex h-full flex-col bg-[var(--bg-primary)]">
      <ChannelHeader />
      <div className="flex-1 overflow-hidden">
        <Outlet />
      </div>
    </main>
  );
}
