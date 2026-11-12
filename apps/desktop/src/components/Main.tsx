import { useQuery, useQueryClient } from "@tanstack/react-query";
import { EyeIcon, EyeOffIcon, HistoryIcon, PlusIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import type { Outcome } from "@/bindings/Outcome";
import { DetailPane } from "@/components/DetailPane";
import {
  type NewWorkspace,
  NewWorkspaceDialog,
  OperationsDialog,
} from "@/components/dialogs";
import { ErrorNote } from "@/components/ErrorNote";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  ConflictPanel,
  WorkspaceList,
  conflictsOf,
} from "@/components/WorkspaceList";
import { api } from "@/lib/api";
import { baseName } from "@/lib/format";
import { useErrorText } from "@/lib/text";

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
};

/** One repository: its workspaces, and the detail of the selected one. */
export function Main({ repo }: Props) {
  const { t } = useTranslation();
  const errorText = useErrorText();
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

  function reportError(error: unknown) {
    const { summary, detail } = errorText(error);
    toast.error(summary, { description: detail });
  }

  const run: Run = (action, done) => {
    setBusy(true);
    action()
      .then((outcome) => {
        toast.success(done(outcome.value));
        for (const notice of outcome.notices) toast.info(notice);
      })
      .catch(reportError)
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
      .catch(reportError)
      .finally(() => void queryClient.invalidateQueries());
  }

  if (overview.error && !overview.data) {
    return (
      <main className="flex min-w-0 flex-1 flex-col items-center justify-center p-8">
        <h1 className="mb-3 font-semibold">{baseName(repo)}</h1>
        <ErrorNote error={overview.error} className="max-w-lg text-center" />
      </main>
    );
  }
  if (!overview.data) return null;

  const { statuses, conflicts, watching } = overview.data;
  const selected =
    statuses.find((status) => status.id === selectedId) ?? statuses[0];

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="flex items-center gap-2 border-b px-4 py-2">
        {/* Where the repository is matters rarely: the path shows when the
            pointer rests on the name. */}
        <Tooltip>
          <TooltipTrigger asChild>
            <h1 className="min-w-0 truncate font-semibold">{baseName(repo)}</h1>
          </TooltipTrigger>
          <TooltipContent side="bottom" align="start">
            {repo}
          </TooltipContent>
        </Tooltip>
        <div className="ml-auto flex shrink-0 items-center gap-1.5">
          <Button
            variant="ghost"
            size="sm"
            title={
              watching ? t("header.watchingHint") : t("header.notWatchingHint")
            }
            onClick={() => toggleWatching(!watching)}
          >
            {watching ? <EyeIcon /> : <EyeOffIcon />}
            {watching ? t("header.watching") : t("header.notWatching")}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setShowingOperations(true)}
          >
            <HistoryIcon />
            {t("header.operations")}
          </Button>
          <Button size="sm" disabled={busy} onClick={() => setCreating(true)}>
            <PlusIcon />
            {t("header.newWorkspace")}
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
            (created) => t("toast.created", { name: created.name }),
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
            (undone) =>
              t("toast.undone", { id: undone.id, kind: undone.detail.kind }),
          )
        }
      />
    </div>
  );
}
