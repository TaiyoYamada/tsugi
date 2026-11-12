import { useEffect, useState } from "react";

import { Main } from "@/components/Main";
import { RepoPicker } from "@/components/RepoPicker";
import { api } from "@/lib/api";

const CURRENT_KEY = "tsugi.repo";
const RECENT_KEY = "tsugi.recent";
const MAX_RECENT = 8;

function readRecent(): string[] {
  const stored: unknown = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
  return Array.isArray(stored)
    ? stored.filter((entry): entry is string => typeof entry === "string")
    : [];
}

export default function App() {
  const [repo, setRepo] = useState<string | null>(() =>
    localStorage.getItem(CURRENT_KEY),
  );
  const [recent, setRecent] = useState<string[]>(readRecent);

  // A directory given on the command line wins over the repository that
  // was open last time. That one may be gone by now, and opening it is what
  // starts the watcher.
  useEffect(() => {
    async function start() {
      const launched = await api.launchPath();
      if (launched) {
        open(await api.openRepo(launched));
        return;
      }
      const stored = localStorage.getItem(CURRENT_KEY);
      if (stored) await api.openRepo(stored);
    }
    start().catch(() => close());
  }, []);

  function open(root: string) {
    const next = [root, ...recent.filter((entry) => entry !== root)].slice(
      0,
      MAX_RECENT,
    );
    localStorage.setItem(CURRENT_KEY, root);
    localStorage.setItem(RECENT_KEY, JSON.stringify(next));
    setRecent(next);
    setRepo(root);
  }

  function close() {
    localStorage.removeItem(CURRENT_KEY);
    setRepo(null);
  }

  return repo ? (
    <Main key={repo} repo={repo} onClose={close} />
  ) : (
    <RepoPicker recent={recent} onOpen={open} />
  );
}
