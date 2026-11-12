import type { Conflict } from "@/bindings/Conflict";
import type { Overview } from "@/bindings/Overview";
import type { WorkspaceStatus } from "@/bindings/WorkspaceStatus";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

type Props = {
  overview: Overview;
  selectedId: string | null;
  onSelect: (id: string) => void;
};

type Group = { title: string; statuses: WorkspaceStatus[] };

/** Workspaces grouped by task, in the order the tasks were created. */
function groupByTask(overview: Overview): Group[] {
  const groups: Group[] = overview.tasks.map((task) => ({
    title: task.title,
    statuses: overview.statuses.filter(
      (status) => status.task_id === task.id,
    ),
  }));
  groups.push({
    title: "No task",
    statuses: overview.statuses.filter((status) => status.task_id === null),
  });
  return groups.filter((group) => group.statuses.length > 0);
}

export function conflictsOf(conflicts: Conflict[], name: string): Conflict[] {
  return conflicts.filter(
    (conflict) =>
      conflict.workspace === name ||
      (!conflict.other_is_base && conflict.other === name),
  );
}

export function WorkspaceList({ overview, selectedId, onSelect }: Props) {
  return (
    <nav className="flex flex-col gap-4 p-3">
      {groupByTask(overview).map((group) => (
        <section key={group.title}>
          <h2 className="mb-1 px-2 text-xs font-medium text-muted-foreground uppercase">
            {group.title}
          </h2>
          <ul className="flex flex-col gap-0.5">
            {group.statuses.map((status) => (
              <li key={status.id}>
                <WorkspaceRow
                  status={status}
                  conflicting={
                    conflictsOf(overview.conflicts, status.name).length > 0
                  }
                  selected={status.id === selectedId}
                  onSelect={() => onSelect(status.id)}
                />
              </li>
            ))}
          </ul>
        </section>
      ))}
    </nav>
  );
}

type RowProps = {
  status: WorkspaceStatus;
  conflicting: boolean;
  selected: boolean;
  onSelect: () => void;
};

function WorkspaceRow({ status, conflicting, selected, onSelect }: RowProps) {
  const session = status.sessions[0];
  return (
    <button
      type="button"
      aria-current={selected}
      onClick={onSelect}
      className={cn(
        "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-muted",
        selected && "bg-muted font-medium",
      )}
    >
      <span
        title={
          session
            ? `${session.agent} is ${session.state}`
            : "no agent session found"
        }
        className={cn(
          "size-2 shrink-0 rounded-full",
          session?.state === "working" && "animate-pulse bg-amber-500",
          session?.state === "idle" && "bg-emerald-500",
          !session && "border border-muted-foreground/40",
        )}
      />
      <span className="truncate">{status.name}</span>
      {status.is_main && (
        <span className="shrink-0 text-xs text-muted-foreground">main</span>
      )}
      <span className="ml-auto flex shrink-0 items-center gap-1.5 text-xs">
        {conflicting && <Badge variant="destructive">conflict</Badge>}
        {status.changes && status.changes.files > 0 && (
          <>
            <span className="text-emerald-600 dark:text-emerald-400">
              +{status.changes.insertions}
            </span>
            <span className="text-red-600 dark:text-red-400">
              -{status.changes.deletions}
            </span>
          </>
        )}
      </span>
    </button>
  );
}

export function ConflictPanel({ conflicts }: { conflicts: Conflict[] }) {
  if (conflicts.length === 0) return null;
  return (
    <section className="border-t p-3">
      <h2 className="mb-2 px-2 text-xs font-medium text-destructive uppercase">
        Conflicts
      </h2>
      <ul className="flex flex-col gap-2 px-2 text-xs">
        {conflicts.map((conflict) => (
          <li key={`${conflict.workspace}/${conflict.other}`}>
            <p className="font-medium">
              {conflict.workspace} × {conflict.other}
              {conflict.other_is_base && " (base)"}
            </p>
            <p className="truncate font-mono text-muted-foreground">
              {conflict.files.join(", ")}
            </p>
          </li>
        ))}
      </ul>
    </section>
  );
}
