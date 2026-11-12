import { useQuery } from "@tanstack/react-query";
import { type FormEvent, type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";

import type { Overview } from "@/bindings/Overview";
import type { WorkspaceStatus } from "@/bindings/WorkspaceStatus";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api } from "@/lib/api";
import { useAgo } from "@/lib/text";

type DialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

type ConfirmProps = DialogProps & {
  title: string;
  description: ReactNode;
  confirmLabel: string;
  destructive?: boolean;
  onConfirm: () => void;
};

/** Asks before something is overwritten or removed. */
export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel,
  destructive = false,
  onConfirm,
}: ConfirmProps) {
  const { t } = useTranslation();
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button
            variant={destructive ? "destructive" : "default"}
            onClick={() => {
              onOpenChange(false);
              onConfirm();
            }}
          >
            {confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

export type NewWorkspace = {
  name: string;
  /** Title of a new task, or id of an existing one. */
  task: { kind: "none" } | { kind: "existing"; id: string } | { kind: "new"; title: string };
  base: string | null;
};

type NewWorkspaceProps = DialogProps & {
  overview: Overview;
  onCreate: (workspace: NewWorkspace) => void;
};

const NO_TASK = "none";
const NEW_TASK = "new";

export function NewWorkspaceDialog({
  open,
  onOpenChange,
  overview,
  onCreate,
}: NewWorkspaceProps) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [task, setTask] = useState(NO_TASK);
  const [title, setTitle] = useState("");
  const [base, setBase] = useState("");
  const openTasks = overview.tasks.filter((entry) => entry.closed_at === null);

  function submit(event: FormEvent) {
    event.preventDefault();
    onCreate({
      name: name.trim(),
      task:
        task === NO_TASK
          ? { kind: "none" }
          : task === NEW_TASK
            ? { kind: "new", title: title.trim() }
            : { kind: "existing", id: task },
      base: base.trim() === "" ? null : base.trim(),
    });
    onOpenChange(false);
    setName("");
    setTitle("");
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form onSubmit={submit} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>{t("newWorkspace.title")}</DialogTitle>
            <DialogDescription>{t("newWorkspace.description")}</DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="workspace-name">{t("newWorkspace.name")}</Label>
            <Input
              id="workspace-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="login-fix"
              pattern="[a-z0-9][a-z0-9._\-]*"
              title={t("newWorkspace.nameRule")}
              required
              autoFocus
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="workspace-task">{t("newWorkspace.task")}</Label>
            <select
              id="workspace-task"
              value={task}
              onChange={(event) => setTask(event.target.value)}
              className="h-8 rounded-md border bg-transparent px-2 text-sm"
            >
              <option value={NO_TASK}>{t("newWorkspace.noTask")}</option>
              {openTasks.map((entry) => (
                <option key={entry.id} value={entry.id}>
                  {entry.title}
                </option>
              ))}
              <option value={NEW_TASK}>{t("newWorkspace.newTask")}</option>
            </select>
            {task === NEW_TASK && (
              <Input
                aria-label={t("newWorkspace.newTaskTitle")}
                value={title}
                onChange={(event) => setTitle(event.target.value)}
                placeholder={t("newWorkspace.newTaskPlaceholder")}
                required
              />
            )}
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="workspace-base">{t("newWorkspace.base")}</Label>
            <Input
              id="workspace-base"
              value={base}
              onChange={(event) => setBase(event.target.value)}
              placeholder={overview.default_base}
            />
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              {t("common.cancel")}
            </Button>
            <Button type="submit">{t("newWorkspace.create")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

type PublishProps = DialogProps & {
  status: WorkspaceStatus;
  onPublish: (message: string | null, verify: boolean) => void;
};

export function PublishDialog({
  open,
  onOpenChange,
  status,
  onPublish,
}: PublishProps) {
  const { t } = useTranslation();
  const [message, setMessage] = useState("");
  const [verify, setVerify] = useState(true);
  const fallback = status.task?.title ?? `Publish ${status.name}`;

  function submit(event: FormEvent) {
    event.preventDefault();
    onPublish(message.trim() === "" ? null : message.trim(), verify);
    onOpenChange(false);
    setMessage("");
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <form onSubmit={submit} className="flex flex-col gap-4">
          <DialogHeader>
            <DialogTitle>
              {t("publish.title", { name: status.name, base: status.base })}
            </DialogTitle>
            <DialogDescription>
              {t("publish.description", { base: status.base })}
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="publish-message">{t("publish.message")}</Label>
            <Input
              id="publish-message"
              value={message}
              onChange={(event) => setMessage(event.target.value)}
              placeholder={fallback}
              autoFocus
            />
          </div>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={verify}
              onChange={(event) => setVerify(event.target.checked)}
            />
            {t("publish.verify")}
          </label>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
            >
              {t("common.cancel")}
            </Button>
            <Button type="submit">{t("publish.submit")}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

type OperationsProps = DialogProps & {
  repo: string;
  onUndo: () => void;
};

/** The operation log, newest first, with undo for the latest operation. */
export function OperationsDialog({
  open,
  onOpenChange,
  repo,
  onUndo,
}: OperationsProps) {
  const { t } = useTranslation();
  const ago = useAgo();
  const operations = useQuery({
    queryKey: ["operations", repo],
    queryFn: () => api.operations(repo, 50),
    enabled: open,
  });
  const undoable = operations.data?.find(
    (operation) =>
      operation.state === "done" && operation.detail.kind !== "undo",
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t("operations.title")}</DialogTitle>
          <DialogDescription>{t("operations.description")}</DialogDescription>
        </DialogHeader>
        <ul className="max-h-96 divide-y overflow-y-auto rounded-lg border text-sm">
          {operations.data?.length === 0 && (
            <li className="p-3 text-muted-foreground">{t("operations.empty")}</li>
          )}
          {operations.data?.map((operation) => (
            <li key={operation.id} className="flex items-baseline gap-3 px-3 py-2">
              <span className="w-10 shrink-0 text-muted-foreground">
                #{operation.id}
              </span>
              <span className="w-24 shrink-0 font-mono text-xs">
                {operation.detail.kind}
              </span>
              <span className="w-20 shrink-0 text-xs">
                {t(`operations.${operation.state}`)}
              </span>
              <span className="min-w-0 flex-1 text-xs text-muted-foreground">
                {operation.note}
              </span>
              <span className="shrink-0 text-xs text-muted-foreground">
                {ago(operation.started_at)}
              </span>
            </li>
          ))}
        </ul>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("common.close")}
          </Button>
          <Button
            disabled={!undoable}
            onClick={() => {
              onOpenChange(false);
              onUndo();
            }}
          >
            {undoable
              ? t("operations.undo", {
                  id: undoable.id,
                  kind: undoable.detail.kind,
                })
              : t("operations.nothingToUndo")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
