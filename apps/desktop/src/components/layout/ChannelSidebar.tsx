import { useLocation } from "react-router";
import { ServerHeader } from "../guild/ServerHeader";
import { ChannelList } from "../guild/ChannelList";
import { UserPanel } from "../members/UserPanel";
import { DmList } from "../sidebar/DmList";

export function ChannelSidebar() {
  const location = useLocation();
  const isDmView = location.pathname.startsWith("/app/dm");

  return (
    <div
      data-testid="channel-sidebar"
      className="flex h-full flex-col bg-[var(--bg-secondary)]"
    >
      {isDmView ? (
        <DmList />
      ) : (
        <>
          <ServerHeader />
          <ChannelList />
        </>
      )}
      <UserPanel />
    </div>
  );
}
