// Fake answers for the commands of the Rust side, so that the screens can
// be worked on in a browser: `VITE_MOCK=1 pnpm dev`. Not part of a build.

import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

import type { Detail } from "@/bindings/Detail";
import type { Diff } from "@/bindings/Diff";
import type { Operation } from "@/bindings/Operation";
import type { Overview } from "@/bindings/Overview";
import type { Snapshot } from "@/bindings/Snapshot";
import type { Trigger } from "@/bindings/Trigger";
import type { WorkspaceStatus } from "@/bindings/WorkspaceStatus";

const ROOT = "/work/shop";
/** A second, quiet repository, to show the list of repositories. */
const OTHER = "/work/blog";
const now = () => Math.floor(Date.now() / 1000);

const PATCH = `diff --git a/src/login.ts b/src/login.ts
index 1111111..2222222 100644
--- a/src/login.ts
+++ b/src/login.ts
@@ -12,7 +12,9 @@ export async function login(form: LoginForm) {
   const user = await findUser(form.email);
-  if (!user) return null;
+  if (!user) {
+    throw new LoginError("unknown email");
+  }
   const ok = await verify(form.password, user.hash);
   return ok ? user : null;
 }
diff --git a/src/errors.ts b/src/errors.ts
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/src/errors.ts
@@ -0,0 +1,3 @@
+export class LoginError extends Error {
+  name = "LoginError";
+}`;

const diff: Diff = {
  from: "a".repeat(40),
  to: "b".repeat(40),
  stat: { files: 2, insertions: 6, deletions: 1 },
  files: [
    { path: "src/login.ts", insertions: 3, deletions: 1 },
    { path: "src/errors.ts", insertions: 3, deletions: 0 },
  ],
  patch: PATCH,
};

function snapshot(workspace: string, seq: number, trigger: Trigger, age: number): Snapshot {
  return {
    id: `${workspace}-${seq}`.padEnd(40, "0"),
    workspace_id: workspace,
    seq,
    tree: "c".repeat(40),
    head: "d".repeat(40),
    branch: `refs/heads/tsugi/${workspace}`,
    trigger,
    created_at: now() - age,
  };
}

function status(
  name: string,
  task: { id: string; title: string } | null,
  extra: Partial<WorkspaceStatus>,
): WorkspaceStatus {
  return {
    id: name,
    name,
    task_id: task?.id ?? null,
    path: `${ROOT}.tsugi/${name}`,
    base: "main",
    origin: "created",
    created_at: now() - 7200,
    removed_at: null,
    task: task
      ? { ...task, created_at: now() - 7200, closed_at: null }
      : null,
    branch: `refs/heads/tsugi/${name}`,
    head: "d".repeat(40),
    is_main: false,
    latest_snapshot: snapshot(name, 12, "watch", 40),
    changes: { files: 2, insertions: 6, deletions: 1 },
    sessions: [],
    ...extra,
  };
}

const login = { id: "t1", title: "Report unknown emails on login" };
const search = { id: "t2", title: "Add product search" };

let statuses: WorkspaceStatus[] = [
  status("shop", null, {
    path: ROOT,
    branch: "refs/heads/main",
    origin: "adopted",
    is_main: true,
    changes: { files: 0, insertions: 0, deletions: 0 },
  }),
  status("login-errors", login, {
    sessions: [
      {
        agent: "claude",
        id: "s1",
        state: "working",
        updated_at: now() - 20,
        title: "Report unknown emails on login",
        last_message: "The error class is in place. Running the login tests now.",
      },
    ],
  }),
  status("login-errors-2", login, {
    changes: { files: 3, insertions: 41, deletions: 8 },
    sessions: [
      {
        agent: "codex",
        id: "s2",
        state: "idle",
        updated_at: now() - 600,
        title: null,
        last_message: "Done. All 42 tests pass.",
      },
    ],
  }),
  status("search", search, { changes: { files: 9, insertions: 210, deletions: 14 } }),
];

const operations: Operation[] = [
  {
    id: 3,
    state: "done",
    detail: { kind: "ws_new", workspace_id: "search", name: "search", path: `${ROOT}.tsugi/search`, branch: "tsugi/search" },
    started_at: now() - 3000,
    finished_at: now() - 3000,
    note: null,
  },
  {
    id: 2,
    state: "failed",
    detail: { kind: "sync", workspace_id: "login-errors", pre: "e".repeat(40), onto: "f".repeat(40) },
    started_at: now() - 5000,
    finished_at: now() - 5000,
    note: "merge conflict in 1 file(s): src/login.ts",
  },
];

function overview(repo: string): Overview {
  if (repo === OTHER) {
    return {
      root: OTHER,
      default_base: "main",
      watching: true,
      statuses: [
        status("blog", null, {
          path: OTHER,
          branch: "refs/heads/main",
          origin: "adopted",
          is_main: true,
          changes: { files: 0, insertions: 0, deletions: 0 },
        }),
      ],
      conflicts: [],
      tasks: [],
    };
  }
  return {
    root: ROOT,
    default_base: "main",
    watching: true,
    statuses,
    conflicts: [
      {
        workspace: "login-errors",
        other: "login-errors-2",
        other_is_base: false,
        files: ["src/login.ts"],
      },
    ],
    tasks: [login, search].map((task) => ({ ...task, created_at: now() - 7200, closed_at: null })),
  };
}

function detail(workspace: string): Detail {
  const triggers: Trigger[] = ["watch", "watch", "pre-sync", "watch", "manual", "auto"];
  return {
    snapshots: triggers.map((trigger, index) =>
      snapshot(workspace, 12 - index, trigger, 40 + index * 400),
    ),
    diff,
  };
}

/** Answers every command the app sends. */
export function installMock() {
  // Start with two repositories open; `#picker` starts with none.
  if (location.hash === "#picker") {
    localStorage.setItem("tsugi.repos", "[]");
  } else if (localStorage.getItem("tsugi.repos") === null) {
    localStorage.setItem("tsugi.repos", JSON.stringify([ROOT, OTHER]));
    localStorage.setItem("tsugi.repo", ROOT);
  }

  mockWindows("main");
  mockIPC((command, payload) => {
    const args = new Map<string, unknown>(Object.entries(payload ?? {}));
    const done = <T,>(value: T) => ({ value, notices: [] });
    const workspace = String(args.get("workspace") ?? "");
    switch (command) {
      case "launch_path":
      case "plugin:window|set_theme":
      case "plugin:window|set_background_color":
        return null;
      case "plugin:event|listen":
        return 0;
      case "plugin:event|unlisten":
        return null;
      case "plugin:app|version":
        return "0.1.0";
      case "plugin:dialog|open":
        return "/work/docs";
      case "open_repo":
      case "init_repo":
        return args.get("path");
      case "close_repo":
        return null;
      case "summary":
        if (args.get("repo") === ROOT) {
          return { workspaces: statuses.length, working: 1, conflicts: 1 };
        }
        if (args.get("repo") === OTHER) {
          return { workspaces: 1, working: 0, conflicts: 0 };
        }
        throw { code: "not_a_repository", message: "not inside a git repository: /work/docs" };
      case "overview":
        if (args.get("repo") !== ROOT && args.get("repo") !== OTHER) {
          throw { code: "not_a_repository", message: "not inside a git repository: /work/docs" };
        }
        return overview(String(args.get("repo")));
      case "detail":
        return detail(workspace);
      case "diff":
        return args.get("from") === args.get("to") ? { ...diff, patch: "", files: [] } : diff;
      case "operations":
        return operations;
      case "set_watching":
        return args.get("on");
      case "snapshot":
        return done({ snapshot: snapshot(workspace, 13, "manual", 0), created: true });
      case "restore":
        return done({ previous: snapshot(workspace, 13, "pre-restore", 0) });
      case "workspace_sync":
        throw { code: "conflict", message: "merge conflict in 1 file(s): src/login.ts" };
      case "publish":
        return done({ base: "main", commit: "9".repeat(40) });
      case "workspace_remove":
        statuses = statuses.filter((entry) => entry.id !== workspace);
        return done({});
      case "task_new":
        return done({ id: "t3", title: args.get("title") });
      case "workspace_new": {
        const created = status(String(args.get("name")), null, { changes: null, latest_snapshot: null });
        statuses = [...statuses, created];
        return done(created);
      }
      case "undo":
        return done(operations[0]);
      default:
        throw { code: "refused", message: `the mock does not know \`${command}\`` };
    }
  });
}
