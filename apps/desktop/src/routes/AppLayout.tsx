import { useEffect, useRef } from "react";
import { useNavigate, useLocation } from "react-router";
import { useAppStore } from "../store";
import { seedStores } from "../mock/seed";
import { AppShell } from "../components/layout/AppShell";
import { ModalRoot } from "../components/modals/ModalRoot";

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
        guildChannelIds.find((cid) => state.channelsById[cid]?.channelType === "text") ??
        guildChannelIds[0];
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
