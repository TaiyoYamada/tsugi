import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";

import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useDark, useSettings } from "@/lib/settings-store";

import App from "./App";
import "./index.css";

// `VITE_MOCK=1 pnpm dev` runs the screens in a browser, without Rust.
if (import.meta.env.VITE_MOCK) {
  const { installMock } = await import("./dev/mock");
  installMock();
}

// `--background` of index.css, light and dark, as the window wants it.
const WINDOW_BACKGROUND = { light: "#ffffff", dark: "#0a0a0a" };

// A failed command is shown as it is. Retrying would only delay the message.
const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

const root = document.getElementById("root");
if (!root) throw new Error("index.html has no #root element");

function Root() {
  const { theme } = useSettings();
  const dark = useDark();
  // The window follows too. Its theme decides the colour of the title and
  // of the dialogs of the system; its background is what the title bar,
  // which is transparent, shows.
  useEffect(() => {
    const appWindow = getCurrentWindow();
    void appWindow.setTheme(theme === "system" ? null : theme);
    void appWindow.setBackgroundColor(
      dark ? WINDOW_BACKGROUND.dark : WINDOW_BACKGROUND.light,
    );
  }, [theme, dark]);
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <App />
        <Toaster position="bottom-right" theme={theme} />
      </TooltipProvider>
    </QueryClientProvider>
  );
}

createRoot(root).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
