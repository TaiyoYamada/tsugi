# Command line interface

Every command accepts `--json` and `-C <dir>`.

With `--json` a command prints exactly one JSON document on standard output.
On success the exit status is 0 and the document is the result described
below. On failure the exit status is 1 and the document is

```json
{ "error": { "code": "workspace_not_found", "message": "no workspace matches `nope`" } }
```

Notes that are not part of the result, such as an interrupted operation that
was just resolved, go to standard error as lines starting with `note: `.

The JSON output is the interface for programs and agents. Fields are added
in minor versions; fields are removed or changed only in major versions. The
text printed without `--json` is for people and can change at any time.

## Selecting things

A workspace is named by its name, its id, or a prefix of its id that only
one workspace has. Where a command takes an optional workspace and none is
given, it uses the workspace that contains the current directory.

A snapshot is named `<workspace>@<seq>`, `<workspace>@latest`, or
`<workspace>` (its latest). Inside a workspace, `@<seq>` is enough.

A task is named by its id, a unique prefix of its id, or its exact title.

## Commands

| Command | Result |
|---|---|
| `tsugi init` | `{ root, workspaces: Workspace[] }` |
| `tsugi status`, `tsugi ws ls` | `WorkspaceStatus[]` |
| `tsugi task new <title>` | `Task` |
| `tsugi task ls` | `Task[]` |
| `tsugi task close <task>` | `Task` |
| `tsugi ws new [name] [--task T] [--base B] [-n N]` | `Workspace[]`, one per workspace created |
| `tsugi ws path [workspace]` | `{ path }` |
| `tsugi ws rm [workspace]` | `Removal` |
| `tsugi ws sync [workspace]` | `{ workspace: Workspace, onto, previous: Snapshot }` |
| `tsugi snapshot [workspace]` | `{ snapshot: Snapshot, created }` |
| `tsugi log [workspace] [-n N]` | `Snapshot[]`, newest first |
| `tsugi diff [workspace] [--from S] [--to S] [--stat]` | `Diff` |
| `tsugi restore <snapshot>` | `{ workspace: Workspace, restored: Snapshot, previous: Snapshot, branch_moved }` |
| `tsugi conflicts` | `Conflict[]` |
| `tsugi publish [workspace] [-m message] [--no-verify]` | `PublishOutcome` |
| `tsugi op log [-n N]` | `Operation[]`, newest first |
| `tsugi undo` | `Operation`, the one that was undone |
| `tsugi watch start` | `{ running: true, started }` |
| `tsugi watch stop` | `{ running: false, stopped }` |
| `tsugi watch status` | `{ running }` |
| `tsugi gc` | `{ deleted, kept }` |
| `tsugi uninstall` | `{ refs_deleted, worktrees: string[] }` |

`tsugi watch run` runs the watcher in the foreground and prints a line per
event. `tsugi` without a command opens the terminal app.

Notes on single commands:

- `ws new` without a name derives one from the title of the task, or uses
  `ws`. With `-n` greater than 1, a number is appended to the name.
- `diff` without `--from` starts at the commit where the workspace left its
  base branch; without `--to` it ends at the current content.
- `snapshot` answers `created: false` and the latest snapshot when nothing
  changed since.
- `publish` uses the title of the task as commit message when `-m` is not
  given, and `Publish <name>` when the workspace has no task.

## Types

Times are Unix seconds. Object ids are full hexadecimal Git object ids.
Branches are full ref names (`refs/heads/main`) except where noted.

```ts
type Task = {
  id: string;
  title: string;
  created_at: number;
  closed_at: number | null;
};

type Workspace = {
  id: string;
  name: string;
  task_id: string | null;
  path: string;
  base: string; // short name of the branch it publishes to
  origin: "created" | "adopted";
  created_at: number;
  removed_at: number | null;
};

type WorkspaceStatus = Workspace & {
  task: Task | null;
  branch: string | null; // null when HEAD is detached
  head: string | null; // null before the first commit
  is_main: boolean; // the main worktree of the repository
  latest_snapshot: Snapshot | null;
  changes: DiffStat | null; // since the base branch; null when unknown
  sessions: Session[]; // most recently active first
};

type Snapshot = {
  id: string; // commit that holds the snapshot
  workspace_id: string;
  seq: number; // position in its workspace, starting at 1
  tree: string;
  head: string | null; // commit HEAD pointed to
  branch: string | null;
  trigger:
    | "watch" | "manual" | "auto"
    | "pre-restore" | "pre-sync" | "pre-publish" | "pre-remove" | "pre-undo";
  created_at: number;
};

type Session = {
  agent: string; // "claude" or "codex"
  id: string;
  state: "working" | "idle";
  updated_at: number;
  title: string | null;
  last_message: string | null;
};

type DiffStat = { files: number; insertions: number; deletions: number };

type Diff = {
  from: string;
  to: string;
  stat: DiffStat;
  // insertions and deletions are null for binary files
  files: { path: string; insertions: number | null; deletions: number | null }[];
  patch: string; // unified diff
};

type Conflict = {
  workspace: string; // name of a workspace
  other: string; // name of the other workspace, or of the base branch
  other_is_base: boolean;
  files: string[];
};

type PublishOutcome = {
  workspace: Workspace;
  base: string; // short name
  commit: string; // now at the tip of the base branch
  stat: DiffStat;
  verifications: {
    command: string;
    passed: boolean;
    exit_code: number | null;
    duration_ms: number;
    cached: boolean; // reused from an earlier run on the same tree
  }[];
  synced: boolean; // false: the workspace changed meanwhile and conflicts
};

type Removal = {
  workspace_id: string;
  name: string;
  path: string;
  branch: string | null;
  head: string | null;
  snapshot: string | null; // the pre-remove snapshot
  branch_deleted: boolean;
};

type Operation = {
  id: number;
  state: "started" | "done" | "failed" | "undone";
  detail:
    | { kind: "task_new"; task_id: string }
    | { kind: "ws_new"; workspace_id: string; name: string; path: string; branch: string }
    | ({ kind: "ws_remove" } & Removal)
    | { kind: "restore"; workspace_id: string; target: string; pre: string | null }
    | { kind: "sync"; workspace_id: string; pre: string | null; onto: string }
    | {
        kind: "publish";
        workspace_id: string;
        base: string;
        pre: string;
        old_tip: string;
        commit: string;
        base_moved: boolean;
      }
    | { kind: "undo"; of: number };
  started_at: number;
  finished_at: number | null;
  note: string | null; // why it failed, or what is left to do
};
```

The same types are generated from the Rust source for the desktop app, in
`apps/desktop/src/bindings/`.

## Error codes

| Code | Meaning |
|---|---|
| `not_a_repository` | The directory is not inside a Git repository. |
| `bare_repository` | The repository has no main worktree. |
| `not_initialized` | `tsugi init` has not been run in this repository. |
| `git_too_old` | Git is older than 2.38. |
| `workspace_not_found`, `snapshot_not_found`, `task_not_found` | Nothing matches the selector. |
| `name_taken`, `invalid_name` | The name of a new workspace cannot be used. |
| `conflict` | A merge has conflicts. The message lists the files. Nothing was changed. |
| `verify_failed` | A command of `publish.verify` failed. The base branch was not changed. |
| `nothing_to_publish` | The workspace has no changes relative to its base branch. |
| `nothing_to_undo` | No finished operation is left to undo. |
| `refused` | A precondition does not hold. The message says which. Nothing was changed. |
| `git_failed` | A git command failed. The message contains what git printed. |
| `invalid_config` | `tsugi.toml` cannot be used. |
| `io`, `database`, `json` | Reading or writing a file or the database failed. |
