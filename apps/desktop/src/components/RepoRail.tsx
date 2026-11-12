import { useQuery } from "@tanstack/react-query";
import { PlusIcon, SettingsIcon, XIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { baseName } from "@/lib/format";
import { cn } from "@/lib/utils";

/**
 * How often the summary of a repository is read again. Slower than the
 * repository on screen: this runs for every open repository.
 */
const REFRESH_MS = 5000;

type Props = {
  repos: string[];
  current: string | null;
  onSelect: (repo: string) => void;
  onRemove: (repo: string) => void;
  onAdd: () => void;
  onSettings: () => void;
};

/**
 * Every open repository, with what needs attention in it, so that an agent
 * that stops or a conflict that appears in a repository that is not on
 * screen is noticed.
 */
export function RepoRail({
  repos,
  current,
  onSelect,
  onRemove,
  onAdd,
  onSettings,
}: Props) {
  const { t } = useTranslation();
  return (
    <aside className="flex w-52 shrink-0 flex-col border-r">
      <h2 className="px-4 pt-3 pb-1 text-xs font-medium text-muted-foreground uppercase">
        {t("rail.title")}
      </h2>
      <ul className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto p-2">
        {repos.map((repo) => (
          <li key={repo}>
            <RailItem
              repo={repo}
              selected={repo === current}
              onSelect={() => onSelect(repo)}
              onRemove={() => onRemove(repo)}
            />
          </li>
        ))}
      </ul>
      <div className="flex items-center gap-1 border-t p-2">
        <Button
          variant="ghost"
          size="sm"
          className="min-w-0 flex-1 justify-start"
          onClick={onAdd}
        >
          <PlusIcon />
          <span className="truncate">{t("rail.add")}</span>
        </Button>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={t("header.settings")}
          onClick={onSettings}
        >
          <SettingsIcon />
        </Button>
      </div>
    </aside>
  );
}

type ItemProps = {
  repo: string;
  selected: boolean;
  onSelect: () => void;
  onRemove: () => void;
};

function RailItem({ repo, selected, onSelect, onRemove }: ItemProps) {
  const { t } = useTranslation();
  const name = baseName(repo);
  const summary = useQuery({
    queryKey: ["summary", repo],
    queryFn: () => api.summary(repo),
    refetchInterval: REFRESH_MS,
  });

  return (
    <div
      className={cn(
        "group relative rounded-md hover:bg-muted",
        selected && "bg-muted",
      )}
    >
      <button
        type="button"
        aria-current={selected}
        title={repo}
        onClick={onSelect}
        className="flex w-full flex-col gap-0.5 px-2 py-1.5 pr-8 text-left"
      >
        <span className={cn("truncate text-sm", selected && "font-medium")}>
          {name}
        </span>
        <span className="flex min-h-4 flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
          {summary.error && (
            <span className="text-destructive">{t("rail.unavailable")}</span>
          )}
          {summary.data && (
            <>
              <span>
                {t("rail.workspaces", { count: summary.data.workspaces })}
              </span>
              {summary.data.working > 0 && (
                <span className="flex items-center gap-1 text-amber-600 dark:text-amber-400">
                  <span className="size-1.5 animate-pulse rounded-full bg-amber-500" />
                  {t("rail.working", { count: summary.data.working })}
                </span>
              )}
              {summary.data.conflicts > 0 && (
                <span className="text-destructive">
                  {t("rail.conflicts", { count: summary.data.conflicts })}
                </span>
              )}
            </>
          )}
        </span>
      </button>
      <Button
        variant="ghost"
        size="icon-xs"
        className="absolute top-1.5 right-1 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
        aria-label={t("rail.remove", { name })}
        onClick={onRemove}
      >
        <XIcon />
      </Button>
    </div>
  );
}
