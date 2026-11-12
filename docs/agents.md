# Using tsugi from an agent

An agent does not have to know about tsugi. A workspace is an ordinary Git
worktree: the agent edits files and runs `git` as usual, and the watcher
saves what it does. This page is for agents, and the people who instruct
them, that want to use tsugi on purpose.

## What an agent can rely on

- Every command prints one JSON document with `--json`, and fails with exit
  status 1 and `{ "error": { "code", "message" } }`. See [cli.md](cli.md).
- Inside a workspace, commands act on that workspace when none is named.
- tsugi never touches the index of a worktree. Whatever the agent has staged
  stays staged.
- Commands that refuse (`conflict`, `refused`, `nothing_to_publish`) have
  changed nothing.

## Instructions to give an agent

Text like the following, in `AGENTS.md` or `CLAUDE.md` of the repository,
is enough.

```markdown
This repository uses tsugi. You work in a tsugi workspace: a Git worktree of
its own.

- Before a change you are not sure about, run `tsugi snapshot`. To go back,
  run `tsugi log` and `tsugi restore @<seq>`.
- Before you say you are done, run `tsugi conflicts --json`. If your
  workspace is listed, run `tsugi ws sync`. If that reports a conflict,
  merge the base branch with git, resolve the conflict, and commit.
- Do not push and do not merge into the base branch. A person publishes
  your work with `tsugi publish`.
- Other workspaces belong to other agents. Do not edit files outside your
  own workspace.
```

## Starting several agents on one task

```
tsugi task new "Report unknown emails on login"
tsugi --json ws new --task "Report unknown emails on login" -n 3
```

The result is an array of workspaces; start one agent in the `path` of
each. `tsugi status` and the apps then show the three side by side, and
`tsugi conflicts` shows where their answers differ in the same lines.
Publish the one you want and remove the others with `tsugi ws rm`.

## Snapshots at the end of every turn

The watcher takes a snapshot once the files of a workspace were quiet for
1.5 seconds. To also mark the end of every turn of Claude Code, call
`tsugi snapshot` from a `Stop` hook, for example in `.claude/settings.json`:

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [{ "type": "command", "command": "tsugi snapshot" }] }
    ]
  }
}
```

The command exits with 0 and writes nothing new when the workspace has not
changed. Hooks of other agents can call the same command.

## What the apps show about an agent

tsugi reads the session logs that Claude Code (`~/.claude/projects/`) and
Codex (`~/.codex/sessions/`) write, and matches them to workspaces by
working directory. It shows whether the agent is in the middle of a turn
and the last thing it said. It reads these files and never writes them.

A session is matched only when the agent was started in the root directory
of the workspace.
