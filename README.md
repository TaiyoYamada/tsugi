# tsugi

tsugi is a layer on top of Git for repositories where several coding agents
work in parallel. It gives every agent its own worktree, saves what each one
is doing as it happens, shows all of them in one place, and puts finished
work on the main branch as one clean commit.

The repository stays an ordinary Git repository. Agents keep using `git`,
GitHub keeps working, and nobody else on the project has to install
anything.

```
$ tsugi status
WORKSPACE       TASK        BRANCH                CHANGES             AGENT                      SNAPSHOT
shop            -           main                  -                   -                          3 (2h ago)
login-errors    Fix login   tsugi/login-errors    2 file(s) +6 -1     claude working (just now)  12 (just now)
login-errors-2  Fix login   tsugi/login-errors-2  3 file(s) +41 -8    codex idle (10m ago)       7 (9m ago)
search          Add search  tsugi/search          9 file(s) +210 -14  -                          31 (1m ago)
```

## What it is for

Running several agents on one repository runs into three problems.

- Each agent needs its own directory, so you use `git worktree`. Git clients
  show branches, not worktrees, so you cannot see what is going on in them.
- Agents try things, break things and overwrite things. What a workspace
  looked like ten minutes ago is gone unless the agent happened to commit.
- The commits agents make while they work end up in the history of the main
  branch.

tsugi answers them with four things.

| | |
|---|---|
| Workspace | A worktree, its branch and the agent sessions running in it, as one unit. Every worktree of the repository is a workspace, including the ones made by `git worktree add` or by another tool. |
| Snapshot | The content of a workspace at one moment. Taken automatically as files change and before anything is overwritten. Stored as Git objects under `refs/tsugi/`, which `git push` does not send. |
| Publish | Puts the content of a workspace on its base branch as a single commit, after merging in memory and running your verification commands. The commits the agent made stay out of the history. |
| Undo | Every change tsugi makes is recorded first. The most recent one can be undone, and one that was interrupted is cleaned up the next time tsugi runs. |

## Status

Version 0.1. The library, the command line tool and the terminal app run on
macOS and Linux. The desktop app, tsugi desktop, is built and tested on
macOS. Windows is not supported.

## Install

tsugi needs Git 2.38 or newer. The command line tool, which includes the
terminal app, installs on macOS and Linux with

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/TaiyoYamada/tsugi/releases/latest/download/tsugi-installer.sh | sh
```

tsugi desktop is `tsugi-desktop-universal-apple-darwin.dmg` on the
[latest release](https://github.com/TaiyoYamada/tsugi/releases/latest).

To build from source you need Rust 1.90 or newer:

```
git clone https://github.com/TaiyoYamada/tsugi
cd tsugi
cargo install --path crates/tsugi
```

## Use

```
cd your-repository
tsugi init                         # adopts the worktrees that exist
tsugi watch start                  # snapshot workspaces as files change

tsugi task new "Fix login"
tsugi ws new --task "Fix login" -n 2   # two workspaces for the same task
cd "$(tsugi ws path fix-login)"    # start an agent here

tsugi status                       # every workspace, its changes and its agent
tsugi conflicts                    # files on which workspaces would collide
tsugi diff fix-login               # what the workspace changed
tsugi log fix-login                # its snapshots
tsugi restore fix-login@12         # go back to snapshot 12

tsugi publish fix-login            # one commit on the base branch
tsugi undo                         # revert the most recent operation
```

`tsugi` without arguments opens the terminal app: the workspaces grouped by
task on the left, the diff, the snapshots and the agent of the selected
workspace on the right. Press `?` for the keys. While it is open it runs
the watcher itself.

Every command accepts `--json`. The output is specified in
[docs/cli.md](docs/cli.md), and [docs/agents.md](docs/agents.md) describes
how an agent can use tsugi itself.

### Settings

`tsugi.toml` at the root of the repository. Every key is optional.

```toml
[workspace]
# Ignored paths cloned from the main worktree into every new workspace.
# On APFS, Btrfs and XFS the data is shared until one side changes it.
copy = ["node_modules", ".env"]
# Branch that new workspaces start from and publish to.
base = "main"

[snapshot]
# Untracked files larger than this are left out of snapshots.
max_file_bytes = 10485760
# Snapshots older than this are deleted by `tsugi gc`, which the watcher
# runs once an hour. Automatic snapshots older than an hour are thinned to
# one per ten minutes.
keep_days = 14

[publish]
# Must succeed, on the merged result, before the base branch moves.
verify = ["cargo test"]
```

## tsugi desktop

```
cd apps/desktop
pnpm install
pnpm tauri build
```

tsugi desktop shows the same things as the terminal app in a window, for
several repositories at once. The repositories you opened are listed on the
left, each with the number of agents at work and of conflicts in it, so
that something that happens in a repository that is not on screen is
noticed.
The app runs the watcher for every repository on the list.
`tsugi-desktop <dir>` adds a repository directly.

The app is in English and Japanese, and light or dark; both follow the
system unless they are set in the settings (the gear button, or ⌘,). The
settings also offer twelve color themes, each of whose colors can be
changed, the size of the text, the font and layout of diffs, and whether
open repositories are watched.

## What tsugi writes, and how to remove it

- `.git/tsugi/`: a SQLite database with tasks, the names of workspaces and
  the operation log; lock files; logs.
- Refs under `refs/tsugi/`, which keep the snapshots alive.
- Worktrees in `../<repository>.tsugi/`, on branches named `tsugi/<name>`.

`tsugi uninstall` deletes the first two. The worktrees and their branches
are ordinary Git worktrees and branches and are left for you to keep or
remove.

## Limits

- A snapshot holds tracked files and untracked files that are not ignored.
  Ignored files are neither saved nor restored.
- Publish makes one commit out of a workspace. There is no way yet to
  publish a series of commits.
- The state of an agent is read from the session logs of Claude Code and
  Codex. Those formats are not documented by their vendors and can change.
  The logs do not show that an agent waits for approval of a tool call;
  such an agent is reported as working.
- Restoring a snapshot restores files and the branch. It does not rewind the
  conversation of the agent.
- The watcher watches every directory of a workspace. On Linux a workspace
  with a very large ignored directory can exceed the inotify watch limit.
- `git push --mirror` pushes the refs under `refs/tsugi/`.

## How it works

[docs/design.md](docs/design.md) specifies what is stored and what every
operation guarantees, including what happens when a process dies in the
middle of one.

## License

Apache-2.0
