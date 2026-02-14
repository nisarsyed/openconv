import { useState, useEffect } from "react";
import { commands } from "./bindings";

function App() {
  const [status, setStatus] = useState<"loading" | "connected" | "error">(
    "loading",
  );

  useEffect(() => {
    document.documentElement.classList.add("dark");
  }, []);

  useEffect(() => {
    commands
      .healthCheck()
      .then(() => setStatus("connected"))
      .catch((err) => {
        console.error("Health check failed:", err);
        setStatus("error");
      });
  }, []);

  return (
    <div className="bg-gray-900 text-gray-100 min-h-screen flex flex-col items-center justify-center">
      <h1 className="text-4xl font-bold mb-4">OpenConv</h1>
      <div data-testid="status-indicator" className="flex items-center gap-2">
        <span
          className={`inline-block w-3 h-3 rounded-full ${
            status === "connected"
              ? "bg-green-500"
              : status === "error"
                ? "bg-red-500"
                : "bg-gray-500"
          }`}
        />
        <span className="text-sm text-gray-400">
          {status === "connected"
            ? "IPC Connected"
            : status === "error"
              ? "IPC Error"
              : "Connecting..."}
        </span>
      </div>
    </div>
  );
}

export default App;
