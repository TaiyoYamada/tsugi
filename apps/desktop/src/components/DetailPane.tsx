import { useQuery } from "@tanstack/react-query";
import {
  CameraIcon,
  GitMergeIcon,
  RotateCcwIcon,
  Trash2Icon,
  UploadIcon,
} from "lucide-react";
import { useState } from "react";

import type { Conflict } from "@/bindings/Conflict";
import type { Detail } from "@/bindings/Detail";
import type { Snapshot } from "@/bindings/Snapshot";
import type { WorkspaceStatus } from "@/bindings/WorkspaceStatus";
import { DiffView } from "@/components/DiffView";
import { ConfirmDialog, PublishDialog } from "@/components/dialogs";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api, errorMessage } from "@/lib/api";
import { ago, shortBranch, shortId } from "@/lib/format";
import { cn } from "@/lib/utils";

import type { Run } from "./Main";

type Props = {
  repo: string;
  status: WorkspaceStatus;
  conflicts: Conflict[];
  busy: boolean;
  run: Run;
};

type Question = "sync" | "remove" | null;

/** Everything about the selected workspace, and the actions on it. */
export function DetailPane({ repo, status, conflicts, busy, run }: Props) {
  const [question, setQuestion] = useState<Question>(null);
  const [publishing, setPublishing] = useState(false);
  const onBase = status.branch === `refs/heads/${status.base}`;

  const detail = useQuery({
    // The latest snapshot is part of the key: a new snapshot means the
    // detail is out of date.
    queryKey: ["detail", repo, status.id, status.latest_snapshot?.id ?? null],
    queryFn: () => api.detail(repo, status.id),
  });

  return (
    <section className="flex min-w-0 flex-1 flex-col">
      <header className="flex items-center gap-2 border-b px-4 py-2">
        <div className="min-w-0">
          <h2 className="truncate font-medium">{status.name}</h2>
          <p className="truncate text-xs text-muted-foreground">
            {status.branch ? shortBranch(status.branch) : "detached"}
            {!onBase && ` → ${status.base}`}
          </p>
        </div>
        <div className="ml-auto flex shrink-0 gap-1.5">
          <Button
            variant="outline"
            size="sm"
            disabled={busy}
            onClick={() =>
              run(
                () => api.snapshot(repo, status.id),
                (outcome) =>
                  outcome.created
                    ? `Snapshot ${outcome.snapshot.seq} of ${status.name}`
                    : `${status.name} has not changed since snapshot ${outcome.snapshot.seq}`,
              )
            }
          >
            <CameraIcon />
            Snapshot
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={busy || onBase}
            onClick={() => setQuestion("sync")}
          >
            <GitMergeIcon />
            Sync
          </Button>
          <Button
            size="sm"
            disabled={busy || onBase}
            onClick={() => setPublishing(true)}
          >
            <UploadIcon />
            Publish
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={`Remove ${status.name}`}
            disabled={busy || status.is_main}
            onClick={() => setQuestion("remove")}
          >
            <Trash2Icon />
          </Button>
        </div>
      </header>

      <Tabs defaultValue="changes" className="min-h-0 flex-1 gap-0">
        <TabsList variant="line" className="mx-4 mt-2">
          <TabsTrigger value="changes">Changes</TabsTrigger>
          <TabsTrigger value="snapshots">
            Snapshots
            {detail.data && ` (${detail.data.snapshots.length})`}
          </TabsTrigger>
          <TabsTrigger value="info">Info</TabsTrigger>
        </TabsList>

        {detail.error && (
          <p role="alert" className="p-6 text-sm text-destructive">
            {errorMessage(detail.error)}
          </p>
        )}
        <TabsContent value="changes" className="min-h-0 overflow-y-auto">
          {detail.data &&
            (detail.data.diff ? (
              <DiffView
                diff={detail.data.diff}
                empty={`No changes since ${status.base}.`}
              />
            ) : (
              <p className="p-6 text-sm text-muted-foreground">
                Nothing to compare with: the base branch {status.base} does
                not exist or shares no history with this workspace.
              </p>
            ))}
        </TabsContent>
        <TabsContent value="snapshots" className="min-h-0">
          {detail.data && (
            <SnapshotsTab
              repo={repo}
              status={status}
              detail={detail.data}
              busy={busy}
              run={run}
            />
          )}
        </TabsContent>
        <TabsContent value="info" className="min-h-0 overflow-y-auto">
          <InfoTab status={status} conflicts={conflicts} />
        </TabsContent>
      </Tabs>

      <ConfirmDialog
        open={question === "sync"}
        onOpenChange={(open) => !open && setQuestion(null)}
        title={`Merge ${status.base} into ${status.name}?`}
        description={`The files of the workspace receive what happened on ${status.base}, and its branch moves on top of it. A snapshot is taken first.`}
        confirmLabel="Sync"
        onConfirm={() =>
          run(
            () => api.workspaceSync(repo, status.id),
            () => `${status.name} now sits on ${status.base}`,
          )
        }
      />
      <ConfirmDialog
        open={question === "remove"}
        onOpenChange={(open) => !open && setQuestion(null)}
        title={`Remove ${status.name}?`}
        description="The worktree is deleted. Its content stays available as a snapshot until the snapshot expires, and Undo brings the workspace back."
        confirmLabel="Remove"
        destructive
        onConfirm={() =>
          run(
            () => api.workspaceRemove(repo, status.id),
            () => `Removed ${status.name}`,
          )
        }
      />
      <PublishDialog
        open={publishing}
        onOpenChange={setPublishing}
        status={status}
        onPublish={(message, verify) =>
          run(
            () => api.publish(repo, status.id, message, verify),
            (outcome) =>
              `Published ${status.name} to ${outcome.base} as ${shortId(outcome.commit)}`,
          )
        }
      />
    </section>
  );
}

type SnapshotsProps = {
  repo: string;
  status: WorkspaceStatus;
  detail: Detail;
  busy: boolean;
  run: Run;
};

function SnapshotsTab({ repo, status, detail, busy, run }: SnapshotsProps) {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [restoring, setRestoring] = useState<Snapshot | null>(null);
  const latest = detail.snapshots[0];
  const selected =
    detail.snapshots.find((snapshot) => snapshot.id === selectedId) ?? latest;

  // What a restore would undo: the changes made since the snapshot.
  const since = useQuery({
    queryKey: ["diff", repo, selected?.id, latest?.id],
    queryFn: () =>
      selected && latest ? api.diff(repo, selected.id, latest.id) : null,
    enabled: selected !== undefined && latest !== undefined,
  });

  if (!selected) {
    return (
      <p className="p-6 text-sm text-muted-foreground">No snapshots yet.</p>
    );
  }
  return (
    <div className="flex h-full min-h-0">
      <ul className="w-64 shrink-0 overflow-y-auto border-r py-1 text-sm">
        {detail.snapshots.map((snapshot) => (
          <li key={snapshot.id}>
            <button
              type="button"
              aria-current={snapshot.id === selected.id}
              onClick={() => setSelectedId(snapshot.id)}
              className={cn(
                "flex w-full items-baseline gap-2 px-3 py-1.5 text-left hover:bg-muted",
                snapshot.id === selected.id && "bg-muted font-medium",
              )}
            >
              <span className="w-10 shrink-0 text-right tabular-nums">
                {snapshot.seq}
              </span>
              <span className="text-xs text-muted-foreground">
                {snapshot.trigger}
              </span>
              <span className="ml-auto text-xs text-muted-foreground">
                {ago(snapshot.created_at)}
              </span>
            </button>
          </li>
        ))}
      </ul>

      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex items-center gap-3 border-b px-4 py-2 text-sm">
          <p className="min-w-0 truncate">
            Snapshot {selected.seq}
            <span className="text-muted-foreground">
              {" "}
              · {ago(selected.created_at)} ·{" "}
              {selected.head ? shortId(selected.head) : "no commit"}
            </span>
          </p>
          <Button
            variant="outline"
            size="sm"
            className="ml-auto shrink-0"
            disabled={busy || selected.id === latest?.id}
            onClick={() => setRestoring(selected)}
          >
            <RotateCcwIcon />
            Restore
          </Button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {since.data && (
            <DiffView
              diff={since.data}
              empty="This is the current state of the workspace."
            />
          )}
        </div>
      </div>

      <ConfirmDialog
        open={restoring !== null}
        onOpenChange={(open) => !open && setRestoring(null)}
        title={`Restore ${status.name} to snapshot ${restoring?.seq}?`}
        description="The files and the branch of the workspace go back to the snapshot. The current state is kept as a snapshot of its own, so this can be undone."
        confirmLabel="Restore"
        onConfirm={() => {
          const target = restoring;
          if (!target) return;
          run(
            () => api.restore(repo, status.id, target.id),
            (outcome) =>
              `Restored ${status.name} to snapshot ${target.seq}; the state before is snapshot ${outcome.previous.seq}`,
          );
        }}
      />
    </div>
  );
}

function InfoTab({
  status,
  conflicts,
}: {
  status: WorkspaceStatus;
  conflicts: Conflict[];
}) {
  const fields: [string, string][] = [
    ["Path", status.path],
    ["Branch", status.branch ? shortBranch(status.branch) : "detached"],
    ["Base", status.base],
    ["Task", status.task?.title ?? "none"],
    [
      "Changes",
      status.changes
        ? `${status.changes.files} file(s), +${status.changes.insertions} -${status.changes.deletions}`
        : "unknown",
    ],
  ];
  return (
    <div className="flex flex-col gap-6 p-4 text-sm">
      <dl className="grid grid-cols-[6rem_1fr] gap-x-4 gap-y-1.5">
        {fields.map(([name, value]) => (
          <div key={name} className="contents">
            <dt className="text-muted-foreground">{name}</dt>
            <dd className="min-w-0 break-all select-text">{value}</dd>
          </div>
        ))}
      </dl>

      {conflicts.length > 0 && (
        <section>
          <h3 className="mb-1 font-medium text-destructive">Conflicts</h3>
          <ul className="flex flex-col gap-1">
            {conflicts.map((conflict) => (
              <li key={`${conflict.workspace}/${conflict.other}`}>
                with{" "}
                {conflict.workspace === status.name
                  ? conflict.other
                  : conflict.workspace}
                :{" "}
                <span className="font-mono text-xs">
                  {conflict.files.join(", ")}
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}

      <section>
        <h3 className="mb-1 font-medium">Agents</h3>
        {status.sessions.length === 0 && (
          <p className="text-muted-foreground">
            No agent session was found for this workspace.
          </p>
        )}
        <ul className="flex flex-col gap-3">
          {status.sessions.map((session) => (
            <li key={`${session.agent}/${session.id}`} className="rounded-lg border p-3">
              <p>
                <span className="font-medium">{session.agent}</span>{" "}
                <span
                  className={
                    session.state === "working"
                      ? "text-amber-600 dark:text-amber-400"
                      : "text-emerald-600 dark:text-emerald-400"
                  }
                >
                  {session.state}
                </span>{" "}
                <span className="text-muted-foreground">
                  · {ago(session.updated_at)}
                </span>
              </p>
              {session.title && <p className="mt-1">{session.title}</p>}
              {session.last_message && (
                <p className="mt-1 whitespace-pre-wrap text-muted-foreground">
                  {session.last_message}
                </p>
              )}
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
