import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";

import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useActiveTheme, useSettings } from "@/lib/settings-store";

import App from "./App";
import "./index.css";

// `VITE_MOCK=1 pnpm dev` runs the screens in a browser, without Rust.
if (import.meta.env.VITE_MOCK) {
  const { installMock } = await import("./dev/mock");
  installMock();
}

// A failed command is shown as it is. Retrying would only delay the message.
const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

const root = document.getElementById("root");
if (!root) throw new Error("index.html has no #root element");

function Root() {
  const { theme: mode } = useSettings();
  const { theme, colors } = useActiveTheme();
  // The window follows too. Its theme decides the colour of the title and
  // of the dialogs of the system; its background is what the title bar,
  // which is transparent, shows.
  useEffect(() => {
    const appWindow = getCurrentWindow();
    void appWindow.setTheme(mode === "system" ? null : theme.kind);
    void appWindow.setBackgroundColor(colors.background);
  }, [mode, theme.kind, colors.background]);
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <App />
        <Toaster position="bottom-right" theme={theme.kind} />
      </TooltipProvider>
    </QueryClientProvider>
  );
}

createRoot(root).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
