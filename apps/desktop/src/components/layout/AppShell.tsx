import { useEffect } from "react";
import { Group, Panel, Separator, usePanelRef } from "react-resizable-panels";
import { useAppStore } from "../../store";
import { GuildSidebar } from "./GuildSidebar";
import { ChannelSidebar } from "./ChannelSidebar";
import { MainContent } from "./MainContent";
import { MemberList } from "./MemberList";
import { DragRegion } from "./DragRegion";
import { useResponsiveCollapse } from "../../hooks/useResponsiveCollapse";

export function AppShell() {
  const channelSidebarVisible = useAppStore((s) => s.channelSidebarVisible);
  const memberListVisible = useAppStore((s) => s.memberListVisible);

  const channelPanelRef = usePanelRef();
  const memberPanelRef = usePanelRef();

  useResponsiveCollapse();

  useEffect(() => {
    if (channelSidebarVisible) {
      channelPanelRef.current?.expand();
    } else {
      channelPanelRef.current?.collapse();
    }
  }, [channelSidebarVisible, channelPanelRef]);

  useEffect(() => {
    if (memberListVisible) {
      memberPanelRef.current?.expand();
    } else {
      memberPanelRef.current?.collapse();
    }
  }, [memberListVisible, memberPanelRef]);

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden">
      <DragRegion />
      <div className="flex flex-1 overflow-hidden">
        <GuildSidebar />

        <Group orientation="horizontal" className="flex-1">
          <Panel
            panelRef={channelPanelRef}
            defaultSize={20}
            minSize={15}
            collapsible
            collapsedSize={0}
          >
            <ChannelSidebar />
          </Panel>

          <Separator className="w-0.5 bg-[var(--border-subtle)] hover:bg-[var(--bg-accent)] transition-colors" />

          <Panel minSize={30}>
            <MainContent />
          </Panel>

          <Separator className="w-0.5 bg-[var(--border-subtle)] hover:bg-[var(--bg-accent)] transition-colors" />

          <Panel
            panelRef={memberPanelRef}
            defaultSize={20}
            minSize={15}
            collapsible
            collapsedSize={0}
          >
            <aside data-testid="member-list" className="h-full bg-[var(--bg-secondary)]">
              <MemberList />
            </aside>
          </Panel>
        </Group>
      </div>
    </div>
  );
}
