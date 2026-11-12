import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpenIcon } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { api, errorMessage, isCommandError } from "@/lib/api";
import { baseName } from "@/lib/format";

type Props = {
  recent: string[];
  onOpen: (root: string) => void;
};

/** The first screen: choose the repository to work with. */
export function RepoPicker({ recent, onOpen }: Props) {
  const [error, setError] = useState<string | null>(null);
  // A repository that tsugi is not set up in yet, waiting for the answer.
  const [toSetUp, setToSetUp] = useState<string | null>(null);

  async function openPath(path: string) {
    setError(null);
    try {
      onOpen(await api.openRepo(path));
    } catch (failure) {
      if (isCommandError(failure) && failure.code === "not_initialized") {
        setToSetUp(path);
      } else {
        setError(errorMessage(failure));
      }
    }
  }

  async function choose() {
    const path = await openDialog({ directory: true, multiple: false });
    if (path) await openPath(path);
  }

  async function setUp(path: string) {
    setToSetUp(null);
    try {
      onOpen(await api.initRepo(path));
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  return (
    <main className="flex h-screen flex-col items-center justify-center gap-8 p-8">
      <div className="text-center">
        <h1 className="text-3xl font-semibold tracking-tight">tsugi</h1>
        <p className="mt-2 text-muted-foreground">
          Every workspace of a repository, and what the agents in them are
          doing.
        </p>
      </div>

      <Button size="lg" onClick={choose}>
        <FolderOpenIcon />
        Open a repository
      </Button>
      {error && (
        <p role="alert" className="max-w-lg text-center text-sm text-destructive">
          {error}
        </p>
      )}

      {recent.length > 0 && (
        <section className="w-full max-w-lg">
          <h2 className="mb-2 text-xs font-medium text-muted-foreground uppercase">
            Recent
          </h2>
          <ul className="divide-y rounded-lg border">
            {recent.map((path) => (
              <li key={path}>
                <button
                  type="button"
                  className="flex w-full flex-col px-3 py-2 text-left hover:bg-muted"
                  onClick={() => openPath(path)}
                >
                  <span className="text-sm font-medium">{baseName(path)}</span>
                  <span className="truncate text-xs text-muted-foreground">
                    {path}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}

      <Dialog
        open={toSetUp !== null}
        onOpenChange={(open) => !open && setToSetUp(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Set tsugi up in this repository?</DialogTitle>
            <DialogDescription>
              tsugi keeps its data inside the .git directory and in refs under
              refs/tsugi/. The repository stays an ordinary Git repository,
              and `tsugi uninstall` removes everything again.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setToSetUp(null)}>
              Cancel
            </Button>
            <Button onClick={() => toSetUp && setUp(toSetUp)}>Set up</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </main>
  );
}
