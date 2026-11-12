import { useQuery, useQueryClient } from "@tanstack/react-query";
import { EyeIcon, EyeOffIcon, HistoryIcon, PlusIcon, XIcon } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import type { Outcome } from "@/bindings/Outcome";
import { DetailPane } from "@/components/DetailPane";
import {
  type NewWorkspace,
  NewWorkspaceDialog,
  OperationsDialog,
} from "@/components/dialogs";
import { Button } from "@/components/ui/button";
import {
  ConflictPanel,
  WorkspaceList,
  conflictsOf,
} from "@/components/WorkspaceList";
import { api, errorMessage } from "@/lib/api";
import { baseName } from "@/lib/format";

/** How often the state of the workspaces is read again. */
const REFRESH_MS = 2000;

/**
 * Runs a command that changes the repository, tells the user how it went,
 * and reloads what is on screen.
 */
export type Run = <T>(
  action: () => Promise<Outcome<T>>,
  done: (value: T) => string,
) => void;

type Props = {
  repo: string;
  onClose: () => void;
};

export function Main({ repo, onClose }: Props) {
  const queryClient = useQueryClient();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [creating, setCreating] = useState(false);
  const [showingOperations, setShowingOperations] = useState(false);

  const overview = useQuery({
    queryKey: ["overview", repo],
    queryFn: () => api.overview(repo),
    refetchInterval: REFRESH_MS,
  });

  const run: Run = (action, done) => {
    setBusy(true);
    action()
      .then((outcome) => {
        toast.success(done(outcome.value));
        for (const notice of outcome.notices) toast.info(notice);
      })
      .catch((error: unknown) => toast.error(errorMessage(error)))
      .finally(() => {
        setBusy(false);
        void queryClient.invalidateQueries();
      });
  };

  async function create(workspace: NewWorkspace) {
    let taskId: string | null = null;
    if (workspace.task.kind === "existing") taskId = workspace.task.id;
    if (workspace.task.kind === "new") {
      const task = await api.taskNew(repo, workspace.task.title);
      taskId = task.value.id;
    }
    const created = await api.workspaceNew(
      repo,
      workspace.name,
      taskId,
      workspace.base,
    );
    setSelectedId(created.value.id);
    return created;
  }

  function toggleWatching(on: boolean) {
    api
      .setWatching(repo, on)
      .catch((error: unknown) => toast.error(errorMessage(error)))
      .finally(() => void queryClient.invalidateQueries());
  }

  if (overview.error && !overview.data) {
    return (
      <main className="flex h-screen flex-col items-center justify-center gap-4 p-8">
        <p role="alert" className="max-w-lg text-center text-sm text-destructive">
          {errorMessage(overview.error)}
        </p>
        <Button variant="outline" onClick={onClose}>
          Choose another repository
        </Button>
      </main>
    );
  }
  if (!overview.data) return null;

  const { statuses, conflicts, watching } = overview.data;
  const selected =
    statuses.find((status) => status.id === selectedId) ?? statuses[0];

  return (
    <div className="flex h-screen flex-col">
      <header className="flex items-center gap-2 border-b px-4 py-2">
        <div className="min-w-0">
          <h1 className="truncate font-semibold">{baseName(repo)}</h1>
          <p className="truncate text-xs text-muted-foreground">{repo}</p>
        </div>
        <div className="ml-auto flex shrink-0 items-center gap-1.5">
          <Button
            variant="ghost"
            size="sm"
            title={
              watching
                ? "Snapshots are taken as files change. Click to stop."
                : "Snapshots are only taken on demand. Click to watch."
            }
            onClick={() => toggleWatching(!watching)}
          >
            {watching ? <EyeIcon /> : <EyeOffIcon />}
            {watching ? "Watching" : "Not watching"}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setShowingOperations(true)}
          >
            <HistoryIcon />
            Operations
          </Button>
          <Button size="sm" disabled={busy} onClick={() => setCreating(true)}>
            <PlusIcon />
            New workspace
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Close the repository"
            onClick={onClose}
          >
            <XIcon />
          </Button>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className="flex w-72 shrink-0 flex-col border-r">
          <div className="min-h-0 flex-1 overflow-y-auto">
            <WorkspaceList
              overview={overview.data}
              selectedId={selected?.id ?? null}
              onSelect={setSelectedId}
            />
          </div>
          <ConflictPanel conflicts={conflicts} />
        </aside>
        {selected && (
          <DetailPane
            key={selected.id}
            repo={repo}
            status={selected}
            conflicts={conflictsOf(conflicts, selected.name)}
            busy={busy}
            run={run}
          />
        )}
      </div>

      <NewWorkspaceDialog
        open={creating}
        onOpenChange={setCreating}
        overview={overview.data}
        onCreate={(workspace) =>
          run(
            () => create(workspace),
            (created) => `Created ${created.name}`,
          )
        }
      />
      <OperationsDialog
        open={showingOperations}
        onOpenChange={setShowingOperations}
        repo={repo}
        onUndo={() =>
          run(
            () => api.undo(repo),
            (undone) => `Undid operation #${undone.id} (${undone.detail.kind})`,
          )
        }
      />
    </div>
  );
}
