import { Outlet } from "react-router";
import { ChannelHeader } from "../chat/ChannelHeader";

export function MainContent() {
  return (
    <main
      data-testid="main-content"
      className="flex h-full flex-col bg-[var(--bg-primary)]"
    >
      <ChannelHeader />
      <div className="flex flex-1 flex-col overflow-hidden">
        <Outlet />
      </div>
    </main>
  );
}
