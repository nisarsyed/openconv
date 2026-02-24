import { useEffect, useRef } from "react";
import { useNavigate, useLocation } from "react-router";
import { listen } from "@tauri-apps/api/event";
import { useAppStore } from "../store";
import { seedStores } from "../mock/seed";
import { AppShell } from "../components/layout/AppShell";
import { ModalRoot } from "../components/modals/ModalRoot";
import { commands } from "../bindings";

export function AppLayout() {
  const navigate = useNavigate();
  const location = useLocation();
  const seeded = useRef(false);

  const guildIds = useAppStore((s) => s.guildIds);

  // Seed stores on mount if not already populated
  useEffect(() => {
    if (!seeded.current && guildIds.length === 0) {
      seedStores();
      seeded.current = true;
    }
  }, [guildIds.length]);

  // Load notification settings from backend on mount
  useEffect(() => {
    useAppStore.getState().loadNotificationSettings();
  }, []);

  // Track the visible channel for notification suppression
  useEffect(() => {
    const match = location.pathname.match(
      /\/app\/guild\/[^/]+\/channel\/([^/]+)/,
    );
    const dmMatch = location.pathname.match(/\/app\/dm\/([^/]+)/);
    const channelId = match?.[1] ?? dmMatch?.[1] ?? null;
    commands.setVisibleChannel(channelId).catch(() => {});
  }, [location.pathname]);

  // Listen for notification clicks to navigate to the target channel
  useEffect(() => {
    const unlisten = listen<{
      channelId?: string;
      dmChannelId?: string;
    }>("notification:clicked", (event) => {
      if (event.payload.channelId) {
        const channel =
          useAppStore.getState().channelsById[event.payload.channelId];
        if (channel) {
          navigate(
            `/app/guild/${channel.guildId}/channel/${channel.id}`,
          );
        }
      } else if (event.payload.dmChannelId) {
        navigate(`/app/dm/${event.payload.dmChannelId}`);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [navigate]);

  // Redirect from /app to last visited guild/channel
  useEffect(() => {
    if (location.pathname !== "/app" && location.pathname !== "/app/") return;

    const state = useAppStore.getState();
    const guildId = state.lastVisitedGuildId ?? state.guildIds[0];
    if (!guildId) return;

    let channelId = state.lastVisitedChannelByGuild[guildId];
    if (!channelId) {
      const guildChannelIds = state.channelIdsByGuild[guildId] ?? [];
      channelId =
        guildChannelIds.find(
          (cid) => state.channelsById[cid]?.channelType === "text",
        ) ?? guildChannelIds[0];
    }

    if (channelId) {
      navigate(`/app/guild/${guildId}/channel/${channelId}`, { replace: true });
    }
  }, [location.pathname, navigate]);

  return (
    <>
      <AppShell />
      <ModalRoot />
    </>
  );
}
