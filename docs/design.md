# tsugi design

tsugi is a layer on top of Git for repositories where several coding agents
work in parallel. The repository stays an ordinary Git repository. tsugi adds
four things: a unit called the workspace, automatic snapshots of every
workspace, an operation log with undo, and a publish step that turns a
workspace into one clean commit.

This document is the specification of what tsugi stores and what each
operation guarantees. The JSON output of the CLI is specified in
[cli.md](cli.md).

## Promises

1. Work is never lost. Every operation that overwrites files or moves a branch
   takes a snapshot first, and an interrupted operation is detected and
   resolved the next time tsugi runs.
2. The repository stays plain Git. Nobody else on the project has to install
   tsugi, and every `git` command keeps working inside every workspace.
3. The history of the base branch only receives commits that were published
   on purpose.
4. `tsugi uninstall` removes everything tsugi wrote.

## Units

| Unit | Meaning |
|---|---|
| Task | One thing to get done. Groups one or more workspaces. |
| Workspace | A Git worktree, its branch, and the agent sessions running in it. |
| Snapshot | The content of a workspace at one moment. |
| Operation | One change made by tsugi, recorded so that it can be undone. |

Every Git worktree of the repository is a workspace. Worktrees that were
created by `git worktree add` or by another tool are adopted the next time
tsugi runs; the main worktree is a workspace too.

## Git is the only Git implementation

tsugi runs the `git` executable for every Git operation and never reads or
writes repository files itself. Snapshots must respect `.gitignore`,
`.gitattributes`, clean and smudge filters, line ending settings and sparse
checkouts exactly as the user's Git does, and the only implementation that is
guaranteed to do so is that Git. Git 2.38 or newer is required because publish
and conflict detection use `git merge-tree --write-tree`.

## What is stored where

```
<git common dir>/tsugi/
  state.db      SQLite: tasks, workspaces, operations, verification results
  lock          lock file held during every mutating operation
  tmp/          temporary index files
  watch.pid     process id of the watcher, if one is running
  watch.log     output of the watcher

refs/tsugi/ws/<workspace id>/latest         newest snapshot of the workspace
refs/tsugi/ws/<workspace id>/snap/<seq>     one ref per snapshot

<parent of repo>/<repo name>.tsugi/<name>/  worktrees created by tsugi
<repo root>/tsugi.toml                      optional, user-written settings
```

Snapshots live in the Git object database and are found through refs, so they
survive the loss of `state.db`. Refs under `refs/tsugi/` are not pushed by
`git push` unless a refspec asks for them (`git push --mirror` does).

Worktrees created by tsugi live next to the repository and not inside it, so
that file watchers and search tools of the main worktree do not descend into
them, and on the same volume, so that files can be cloned copy-on-write.

## Snapshots

A snapshot is a commit object.

- Its tree holds every tracked file as it is on disk, plus every untracked
  file that is not ignored and is not larger than `snapshot.max_file_bytes`.
- Its only parent is the commit `HEAD` pointed to when the snapshot was taken.
  This keeps commits made by an agent reachable even after the agent amends
  or rebases them away, and it gives Git a merge base between a snapshot and
  any branch.
- Author and committer are `tsugi <tsugi@localhost>`, so that taking a
  snapshot never depends on the user's Git identity.
- The message carries the metadata as trailers:

```
tsugi snapshot

Tsugi-Workspace: 3f9a1c0b77de
Tsugi-Seq: 42
Tsugi-Trigger: watch
Tsugi-Head: 8d1e0f...
Tsugi-Branch: refs/heads/tsugi/login-fix
```

`Tsugi-Trigger` says why the snapshot was taken.

| Trigger | Taken |
|---|---|
| `watch` | by the watcher, after files changed |
| `manual` | by `tsugi snapshot` |
| `auto` | by a command that needed the current content of the workspace, such as `status`, `diff` and `conflicts` |
| `pre-restore`, `pre-sync`, `pre-publish`, `pre-remove`, `pre-undo` | by the operation of that name, before it changes anything |

The tree is built with a temporary index (`GIT_INDEX_FILE`) that starts as a
copy of the worktree's own index. The worktree's index, and therefore whatever
the agent has staged, is never modified.

No snapshot is written when both the tree and `HEAD` are identical to the
latest snapshot of the workspace. Commands that report the state of
workspaces take a snapshot first, so what they report is the content on disk
at that moment and not the content at the time of the last snapshot. The
apps, which ask every two seconds, skip this while a watcher runs: the
latest snapshots are then at most a few seconds old.

The two refs of a new snapshot are written in one `git update-ref --stdin`
transaction that also verifies the previous value of `latest`. Two processes
that race to snapshot the same workspace cannot both succeed with the same
sequence number.

### Restore

`tsugi restore <workspace>@<seq>` makes the files of the workspace identical
to the snapshot and moves the checked-out branch and the index to the
snapshot's `Tsugi-Head`. Ignored files are left alone. A `pre-restore`
snapshot is taken first, so a restore can itself be restored or undone.
If the workspace has a different branch checked out than the snapshot
recorded, only the files are restored and no ref is moved.

### Expiry

`tsugi gc` deletes snapshot refs, and the watcher runs it once an hour.

- Snapshots older than `snapshot.keep_days` are deleted.
- Of the snapshots taken automatically (`watch`, `auto`) more than an hour
  ago, the last one of every ten minutes is kept. An agent at work produces
  a snapshot every few seconds; that detail matters for the last hour.
- The latest snapshot of a workspace that still exists is always kept.
  Snapshots taken on request or before an operation are kept until they are
  older than `snapshot.keep_days`.

Git's own garbage collection then removes the objects that are no longer
reachable.

## Publish

`tsugi publish <workspace>` puts the content of a workspace on its base
branch as a single commit.

1. Take a `pre-publish` snapshot `S`.
2. Merge `S` into the tip `B` of the base branch in memory
   (`git merge-tree --write-tree`). On conflict, stop and list the files;
   nothing has changed.
3. Create the commit `C` with the merged tree and the single parent `B`.
4. Run the commands of `publish.verify` in a temporary detached worktree
   checked out at `C`, with the paths of `workspace.copy` cloned from the
   workspace. A command that exits non-zero stops the publish; the base
   branch has not moved. Results are stored per tree and command, so an
   unchanged tree is not verified twice. The repository lock is released
   during this step: verification can take minutes, and other commands and
   the watcher must not wait for it.
5. Move the base branch from `B` to `C`. If the base branch is checked out in
   a worktree, this is done with `git merge --ff-only` in that worktree so
   that its files follow; otherwise with `git update-ref`, guarded by the old
   value.
6. Sync the workspace: its files become the merged tree and its branch moves
   to `C`. The commits the agent made in the workspace are no longer on any
   branch, but stay reachable from the snapshots. If the workspace changed
   during verification in a way that conflicts with `C`, it is left as it is
   and the user is told; the publish itself has succeeded.

What is published is the content of the workspace: every file that a
snapshot holds, committed or not. Like Jujutsu, tsugi has no notion of
"uncommitted" for a workspace.

The published commit is signed when `commit.gpgsign` is set.

`tsugi ws sync <workspace>` performs the merge with the current base branch
without publishing (steps 1, 2 and 6 with `C` replaced by `B`).

## Conflicts between workspaces

`tsugi conflicts` merges the latest snapshots of every pair of workspaces,
and every workspace with its base branch, in memory and reports the files
that conflict. Nothing is written to any worktree.

## Operations, recovery and undo

Every mutating command

1. takes the repository lock (`tsugi/lock`, an exclusive file lock),
2. resolves operations that were left unfinished by an earlier process,
3. inserts a row in `operations` with state `started`, and takes a lock on
   a file of its own (`tsugi/ops/<id>.lock`),
4. does the work, recording in the row every fact that undo or recovery needs
   as soon as it is known (for example the `pre-restore` snapshot),
5. sets the state to `done`.

An operation whose row says `started` and whose lock file is not locked
belongs to a process that died: the operating system released the lock.
This is how a publish that is being verified, which holds no repository
lock, is told apart from a publish that crashed.

An operation that fails with an error is resolved the same way, at once,
by the process that ran it.

Recovery of an unfinished operation depends on its kind.

| Kind | Recovery |
|---|---|
| `ws_new` | Remove the half-created worktree and branch, mark `failed`. |
| `ws_remove` | If the `pre-remove` snapshot exists, finish the removal; otherwise mark `failed`. |
| `restore`, `sync` | Mark `failed`. The files may be partly updated; the pre-operation snapshot is reported so the user can restore it. tsugi does not overwrite files during recovery because an agent may have continued working since the crash. |
| `publish` | If the base branch already points to the new commit, mark `done` and report that the workspace still needs `tsugi ws sync`; otherwise mark `failed`. |

`tsugi undo` reverts the most recent operation that is `done` and not yet
undone.

| Kind | Undo |
|---|---|
| `ws_new` | Remove the workspace (with a snapshot). |
| `ws_remove` | Recreate the worktree and branch and restore the last snapshot. |
| `restore`, `sync` | Restore the pre-operation snapshot. |
| `publish` | Move the base branch back, if it still points to the published commit, and restore the workspace to its `pre-publish` snapshot. |
| `task_new` | Delete the task if it has no workspaces. |

Snapshots, `gc` and `uninstall` are not operations and cannot be undone.

## Removing a workspace

`tsugi ws rm` takes a `pre-remove` snapshot and deletes the worktree. The
branch tsugi created for the workspace is deleted only when every commit on
it is on the base branch already; otherwise it is kept and the user is
told. The snapshots of a removed workspace expire like all others, after
`snapshot.keep_days`. The main worktree cannot be removed.

## The source of truth

Git is the source of truth for everything Git can express: which worktrees
exist, where branches point, what a snapshot contains. `state.db` holds what
Git cannot express: tasks, the name and task of each workspace, the operation
log and verification results. At the start of every command tsugi reconciles
the two: a worktree that Git lists and tsugi does not know is adopted, and a
workspace whose worktree no longer exists is marked removed. Its snapshots are
kept until they expire.

## The watcher

`tsugi watch start` runs a background process per repository; the terminal
app and the desktop app run the same loop inside their own process while
they are open. A lock on `tsugi/watch.lock`
makes sure there is one watcher per repository. It watches the directory of
every workspace and the reflog of `HEAD` in the per-worktree Git directory
(for commits, checkouts and resets). A workspace is snapshotted once its
files were quiet for 1.5 seconds, or 10 seconds after the first change if
they never are. Changes that only concern ignored files are dropped.

The watcher is an optimisation of granularity, not a requirement for safety:
every command that overwrites files takes its own snapshot first, and
`tsugi snapshot` can be run by hand or from an agent hook.

## Agent sessions

tsugi reads, and never writes, the session logs that agents keep.

| Agent | Location | Matching |
|---|---|---|
| Claude Code | `~/.claude/projects/<encoded cwd>/<session id>.jsonl` | The directory name is the workspace path with every character outside `[A-Za-z0-9]` replaced by `-`. |
| Codex | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | `payload.cwd` of the first line (`session_meta`). |

From the tail of a log tsugi derives whether the agent is working or idle and
the last thing it said. The logs do not say whether an agent waits for the
user to approve a tool call; such a session is reported as working. Support for another agent is one more implementation
of the same function; nothing else in tsugi depends on a specific agent.

## Settings

`tsugi.toml` at the root of the repository. Every key is optional.

```toml
[workspace]
# Ignored paths that are cloned (copy-on-write where the filesystem supports
# it) from the main worktree into every new workspace.
copy = ["node_modules", ".env"]
# Branch that new workspaces start from and publish to.
base = "main"

[snapshot]
max_file_bytes = 10485760
keep_days = 14

[publish]
verify = ["cargo test"]
```
