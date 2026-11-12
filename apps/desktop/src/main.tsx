import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useSettings } from "@/lib/settings-store";

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
  const { theme } = useSettings();
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
