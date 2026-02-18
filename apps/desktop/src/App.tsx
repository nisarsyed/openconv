import { useEffect, useRef } from "react";
import { MemoryRouter, Routes, Route, Navigate } from "react-router";
import { useAppStore } from "./store";
import { usePlatform } from "./hooks/usePlatform";
import { LoginPage } from "./routes/LoginPage";
import { RegisterPage } from "./routes/RegisterPage";
import { RecoverPage } from "./routes/RecoverPage";
import { ProtectedRoute } from "./routes/ProtectedRoute";
import { AppLayout } from "./routes/AppLayout";
import { ChannelView } from "./components/chat/ChannelView";
import { UserSettings } from "./components/settings/UserSettings";
import { GuildSettings } from "./components/settings/GuildSettings";

function CatchAllRedirect() {
  const isAuthenticated = useAppStore((state) => state.isAuthenticated);
  return <Navigate to={isAuthenticated ? "/app" : "/login"} replace />;
}

function App() {
  const theme = useAppStore((state) => state.theme);
  const os = usePlatform();
  const initialEntry = useRef(
    useAppStore.getState().isAuthenticated ? "/app" : "/login"
  );

  useEffect(() => {
    if (theme === "dark") {
      document.documentElement.classList.add("dark");
      document.documentElement.classList.remove("light");
    } else {
      document.documentElement.classList.add("light");
      document.documentElement.classList.remove("dark");
    }
  }, [theme]);

  useEffect(() => {
    document.documentElement.style.setProperty(
      "--titlebar-inset",
      os === "macos" ? "2rem" : "0px",
    );
  }, [os]);

  return (
    <MemoryRouter initialEntries={[initialEntry.current]}>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route path="/register" element={<RegisterPage />} />
        <Route path="/recover" element={<RecoverPage />} />
        <Route
          path="/app"
          element={
            <ProtectedRoute>
              <AppLayout />
            </ProtectedRoute>
          }
        >
          <Route path="guild/:guildId/channel/:channelId" element={<ChannelView />} />
          <Route path="settings" element={<UserSettings />} />
          <Route path="guild/:guildId/settings" element={<GuildSettings />} />
          <Route index element={<div>Welcome</div>} />
        </Route>
        <Route path="*" element={<CatchAllRedirect />} />
      </Routes>
    </MemoryRouter>
  );
}

export default App;
