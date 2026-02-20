import { ServerHeader } from "../guild/ServerHeader";
import { ChannelList } from "../guild/ChannelList";
import { UserPanel } from "../members/UserPanel";

export function ChannelSidebar() {
  return (
    <div data-testid="channel-sidebar" className="flex h-full flex-col bg-[var(--bg-secondary)]">
      <ServerHeader />
      <ChannelList />
      <UserPanel />
    </div>
  );
}
