import { useEffect, useRef } from "react";
import { MemoryRouter, Routes, Route, Navigate } from "react-router";
import { useAppStore } from "./store";
import { LoginPage } from "./routes/LoginPage";
import { RegisterPage } from "./routes/RegisterPage";
import { RecoverPage } from "./routes/RecoverPage";
import { ProtectedRoute } from "./routes/ProtectedRoute";

function CatchAllRedirect() {
  const isAuthenticated = useAppStore((state) => state.isAuthenticated);
  return <Navigate to={isAuthenticated ? "/app" : "/login"} replace />;
}

function App() {
  const theme = useAppStore((state) => state.theme);
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

  return (
    <MemoryRouter initialEntries={[initialEntry.current]}>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route path="/register" element={<RegisterPage />} />
        <Route path="/recover" element={<RecoverPage />} />
        <Route
          path="/app/*"
          element={
            <ProtectedRoute>
              <Routes>
                <Route path="guild/:guildId/channel/:channelId" element={<div>Channel View</div>} />
                <Route path="settings" element={<div>User Settings</div>} />
                <Route path="guild/:guildId/settings" element={<div>Guild Settings</div>} />
                <Route index element={<div>Welcome</div>} />
              </Routes>
            </ProtectedRoute>
          }
        />
        <Route path="*" element={<CatchAllRedirect />} />
      </Routes>
    </MemoryRouter>
  );
}

export default App;
