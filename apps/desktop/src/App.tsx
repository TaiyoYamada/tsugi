import { useEffect, useState } from "react";
import { toast } from "sonner";

import { Main } from "@/components/Main";
import { useRepoOpener } from "@/components/RepoOpener";
import { RepoPicker } from "@/components/RepoPicker";
import { RepoRail } from "@/components/RepoRail";
import { SettingsDialog } from "@/components/SettingsDialog";
import { api } from "@/lib/api";
import {
  type RepoList,
  parseRepoList,
  storedStrings,
  withRepo,
  withoutRepo,
} from "@/lib/repos";
import { useErrorText } from "@/lib/text";

const REPOS_KEY = "tsugi.repos";
const CURRENT_KEY = "tsugi.repo";
const RECENT_KEY = "tsugi.recent";
const MAX_RECENT = 8;

export default function App() {
  const errorText = useErrorText();
  const [list, setList] = useState<RepoList>(() =>
    parseRepoList(
      localStorage.getItem(REPOS_KEY),
      localStorage.getItem(CURRENT_KEY),
    ),
  );
  const [recent, setRecent] = useState<string[]>(() =>
    storedStrings(localStorage.getItem(RECENT_KEY)),
  );
  // Shown on the first screen, where there is no other place for it.
  const [openError, setOpenError] = useState<unknown>(null);
  const [showingSettings, setShowingSettings] = useState(false);

  useEffect(() => {
    localStorage.setItem(REPOS_KEY, JSON.stringify(list.repos));
    if (list.current === null) localStorage.removeItem(CURRENT_KEY);
    else localStorage.setItem(CURRENT_KEY, list.current);
  }, [list]);
  useEffect(() => {
    localStorage.setItem(RECENT_KEY, JSON.stringify(recent));
  }, [recent]);

  function open(root: string) {
    setOpenError(null);
    setList((previous) => withRepo(previous, root));
    setRecent((previous) =>
      [root, ...previous.filter((entry) => entry !== root)].slice(0, MAX_RECENT),
    );
  }

  function remove(root: string) {
    // Stops the watcher the app runs for the repository.
    void api.closeRepo(root).catch(() => {});
    setList((previous) => withoutRepo(previous, root));
  }

  const opener = useRepoOpener(open, (error) => {
    if (list.repos.length === 0) {
      setOpenError(error);
    } else {
      const { summary, detail } = errorText(error);
      toast.error(summary, { description: detail });
    }
  });

  // When the app starts: open every repository on the list, which starts
  // their watchers, and the directory given on the command line, if any.
  useEffect(() => {
    for (const repo of list.repos) {
      // A repository that cannot be opened stays on the list, marked as
      // such: its folder may be on a disk that is not connected right now.
      void api.openRepo(repo).catch(() => {});
    }
    void api.launchPath().then((launched) => {
      if (launched) void opener.openPath(launched);
    });
    // Runs once, with the list and the opener of that moment.
  }, []);

  return (
    <>
      {list.current === null ? (
        <RepoPicker
          recent={recent}
          error={openError}
          onChoose={opener.choose}
          onOpenPath={opener.openPath}
          onSettings={() => setShowingSettings(true)}
        />
      ) : (
        <div className="flex h-screen">
          <RepoRail
            repos={list.repos}
            current={list.current}
            onSelect={(repo) =>
              setList((previous) => ({ ...previous, current: repo }))
            }
            onRemove={remove}
            onAdd={opener.choose}
            onSettings={() => setShowingSettings(true)}
          />
          <Main key={list.current} repo={list.current} />
        </div>
      )}
      {opener.dialog}
      <SettingsDialog
        open={showingSettings}
        onOpenChange={setShowingSettings}
      />
    </>
  );
}
