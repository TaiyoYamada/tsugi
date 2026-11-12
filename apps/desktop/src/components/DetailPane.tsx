import { useQuery } from "@tanstack/react-query";
import {
  CameraIcon,
  GitMergeIcon,
  RotateCcwIcon,
  Trash2Icon,
  UploadIcon,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import type { Conflict } from "@/bindings/Conflict";
import type { Detail } from "@/bindings/Detail";
import type { Snapshot } from "@/bindings/Snapshot";
import type { WorkspaceStatus } from "@/bindings/WorkspaceStatus";
import { DiffView } from "@/components/DiffView";
import { ConfirmDialog, PublishDialog } from "@/components/dialogs";
import { ErrorNote } from "@/components/ErrorNote";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { api } from "@/lib/api";
import { shortBranch, shortId } from "@/lib/format";
import { useAgo } from "@/lib/text";
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
  const { t } = useTranslation();
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
            {status.branch ? shortBranch(status.branch) : t("detail.detached")}
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
                  t(outcome.created ? "toast.snapshotTaken" : "toast.unchanged", {
                    name: status.name,
                    seq: outcome.snapshot.seq,
                  }),
              )
            }
          >
            <CameraIcon />
            {t("detail.snapshot")}
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={busy || onBase}
            onClick={() => setQuestion("sync")}
          >
            <GitMergeIcon />
            {t("detail.sync")}
          </Button>
          <Button
            size="sm"
            disabled={busy || onBase}
            onClick={() => setPublishing(true)}
          >
            <UploadIcon />
            {t("detail.publish")}
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t("detail.remove", { name: status.name })}
            disabled={busy || status.is_main}
            onClick={() => setQuestion("remove")}
          >
            <Trash2Icon />
          </Button>
        </div>
      </header>

      <Tabs defaultValue="changes" className="min-h-0 flex-1 gap-0">
        <TabsList variant="line" className="mx-4 mt-2">
          <TabsTrigger value="changes">{t("detail.changes")}</TabsTrigger>
          <TabsTrigger value="snapshots">
            {detail.data
              ? t("detail.snapshotsCount", {
                  count: detail.data.snapshots.length,
                })
              : t("detail.snapshots")}
          </TabsTrigger>
          <TabsTrigger value="info">{t("detail.info")}</TabsTrigger>
        </TabsList>

        {detail.error && <ErrorNote error={detail.error} className="p-6" />}
        <TabsContent value="changes" className="min-h-0 overflow-y-auto">
          {detail.data &&
            (detail.data.diff ? (
              <DiffView
                diff={detail.data.diff}
                empty={t("detail.noChanges", { base: status.base })}
              />
            ) : (
              <p className="p-6 text-sm text-muted-foreground">
                {t("detail.nothingToCompare", { base: status.base })}
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
        title={t("detail.syncTitle", { base: status.base, name: status.name })}
        description={t("detail.syncDescription", { base: status.base })}
        confirmLabel={t("detail.sync")}
        onConfirm={() =>
          run(
            () => api.workspaceSync(repo, status.id),
            () => t("toast.synced", { name: status.name, base: status.base }),
          )
        }
      />
      <ConfirmDialog
        open={question === "remove"}
        onOpenChange={(open) => !open && setQuestion(null)}
        title={t("detail.removeTitle", { name: status.name })}
        description={t("detail.removeDescription")}
        confirmLabel={t("detail.removeConfirm")}
        destructive
        onConfirm={() =>
          run(
            () => api.workspaceRemove(repo, status.id),
            () => t("toast.removed", { name: status.name }),
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
              t("toast.published", {
                name: status.name,
                base: outcome.base,
                commit: shortId(outcome.commit),
              }),
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
  const { t } = useTranslation();
  const ago = useAgo();
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
      <p className="p-6 text-sm text-muted-foreground">
        {t("snapshots.none")}
      </p>
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
            {t("snapshots.title", { seq: selected.seq })}
            <span className="text-muted-foreground">
              {" "}
              · {ago(selected.created_at)} ·{" "}
              {selected.head ? shortId(selected.head) : t("snapshots.noCommit")}
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
            {t("snapshots.restore")}
          </Button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">
          {since.data && (
            <DiffView
              diff={since.data}
              empty={t("snapshots.isCurrent")}
            />
          )}
        </div>
      </div>

      <ConfirmDialog
        open={restoring !== null}
        onOpenChange={(open) => !open && setRestoring(null)}
        title={t("snapshots.restoreTitle", {
          name: status.name,
          seq: restoring?.seq,
        })}
        description={t("snapshots.restoreDescription")}
        confirmLabel={t("snapshots.restore")}
        onConfirm={() => {
          const target = restoring;
          if (!target) return;
          run(
            () => api.restore(repo, status.id, target.id),
            (outcome) =>
              t("toast.restored", {
                name: status.name,
                seq: target.seq,
                previous: outcome.previous.seq,
              }),
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
  const { t } = useTranslation();
  const ago = useAgo();
  const fields: [string, string][] = [
    [t("info.path"), status.path],
    [
      t("info.branch"),
      status.branch ? shortBranch(status.branch) : t("detail.detached"),
    ],
    [t("info.base"), status.base],
    [t("info.task"), status.task?.title ?? t("info.none")],
    [
      t("info.changes"),
      status.changes ? t("info.changesValue", status.changes) : t("info.unknown"),
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
          <h3 className="mb-1 font-medium text-destructive">
            {t("info.conflicts")}
          </h3>
          <ul className="flex flex-col gap-1">
            {conflicts.map((conflict) => (
              <li key={`${conflict.workspace}/${conflict.other}`}>
                {t("info.conflictWith", {
                  other:
                    conflict.workspace === status.name
                      ? conflict.other
                      : conflict.workspace,
                })}{" "}
                <span className="font-mono text-xs">
                  {conflict.files.join(", ")}
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}

      <section>
        <h3 className="mb-1 font-medium">{t("info.agents")}</h3>
        {status.sessions.length === 0 && (
          <p className="text-muted-foreground">{t("info.noSession")}</p>
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
                  {t(`session.${session.state}`)}
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
