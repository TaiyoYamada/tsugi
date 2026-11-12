// The commands of the Rust side (src-tauri/src/lib.rs), typed with the
// types generated from Rust (`pnpm bindings`).

import { invoke } from "@tauri-apps/api/core";

import type { CommandError } from "@/bindings/CommandError";
import type { Detail } from "@/bindings/Detail";
import type { Diff } from "@/bindings/Diff";
import type { Operation } from "@/bindings/Operation";
import type { Outcome } from "@/bindings/Outcome";
import type { Overview } from "@/bindings/Overview";
import type { PublishOutcome } from "@/bindings/PublishOutcome";
import type { Removal } from "@/bindings/Removal";
import type { RestoreOutcome } from "@/bindings/RestoreOutcome";
import type { SnapshotOutcome } from "@/bindings/SnapshotOutcome";
import type { SyncOutcome } from "@/bindings/SyncOutcome";
import type { Task } from "@/bindings/Task";
import type { Workspace } from "@/bindings/Workspace";

export const api = {
  launchPath: () => invoke<string | null>("launch_path"),
  openRepo: (path: string) => invoke<string>("open_repo", { path }),
  initRepo: (path: string) => invoke<string>("init_repo", { path }),
  overview: (repo: string) => invoke<Overview>("overview", { repo }),
  detail: (repo: string, workspace: string) =>
    invoke<Detail>("detail", { repo, workspace }),
  diff: (repo: string, from: string, to: string) =>
    invoke<Diff>("diff", { repo, from, to }),
  setWatching: (repo: string, on: boolean) =>
    invoke<boolean>("set_watching", { repo, on }),
  taskNew: (repo: string, title: string) =>
    invoke<Outcome<Task>>("task_new", { repo, title }),
  workspaceNew: (
    repo: string,
    name: string,
    task: string | null,
    base: string | null,
  ) => invoke<Outcome<Workspace>>("workspace_new", { repo, name, task, base }),
  workspaceRemove: (repo: string, workspace: string) =>
    invoke<Outcome<Removal>>("workspace_remove", { repo, workspace }),
  workspaceSync: (repo: string, workspace: string) =>
    invoke<Outcome<SyncOutcome>>("workspace_sync", { repo, workspace }),
  snapshot: (repo: string, workspace: string) =>
    invoke<Outcome<SnapshotOutcome>>("snapshot", { repo, workspace }),
  restore: (repo: string, workspace: string, snapshot: string) =>
    invoke<Outcome<RestoreOutcome>>("restore", { repo, workspace, snapshot }),
  publish: (
    repo: string,
    workspace: string,
    message: string | null,
    verify: boolean,
  ) =>
    invoke<Outcome<PublishOutcome>>("publish", {
      repo,
      workspace,
      message,
      verify,
    }),
  undo: (repo: string) => invoke<Outcome<Operation>>("undo", { repo }),
  operations: (repo: string, limit: number) =>
    invoke<Operation[]>("operations", { repo, limit }),
};

/** A rejected command carries the error the Rust side returned. */
export function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    typeof value.code === "string" &&
    "message" in value &&
    typeof value.message === "string"
  );
}

export function errorMessage(error: unknown): string {
  if (isCommandError(error)) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}
