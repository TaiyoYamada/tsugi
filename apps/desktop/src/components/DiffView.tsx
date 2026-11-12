import { useMemo } from "react";
import { useTranslation } from "react-i18next";

import type { Diff } from "@/bindings/Diff";
import { type LineKind, parsePatch } from "@/lib/patch";
import { useSettings } from "@/lib/settings-store";
import { cn } from "@/lib/utils";

/** Files longer than this start folded, so a huge diff stays responsive. */
const FOLD_ABOVE_LINES = 800;

const LINE_STYLE: Record<LineKind, string> = {
  add: "bg-added/15",
  delete: "bg-removed/15",
  context: "",
  hunk: "bg-muted text-muted-foreground",
  meta: "text-muted-foreground",
};

const LINE_MARK: Record<LineKind, string> = {
  add: "+",
  delete: "-",
  context: " ",
  hunk: "",
  meta: "",
};

type Props = {
  diff: Diff;
  /** Shown when the diff is empty. */
  empty: string;
};

export function DiffView({ diff, empty }: Props) {
  const { t } = useTranslation();
  const { diffWrap, diffLineNumbers } = useSettings();
  const files = useMemo(() => parsePatch(diff.patch), [diff.patch]);

  if (files.length === 0) {
    return <p className="p-6 text-sm text-muted-foreground">{empty}</p>;
  }
  return (
    <div className="flex flex-col gap-3 p-3">
      <p className="text-xs text-muted-foreground">
        {t("diff.summary", { count: diff.stat.files })}{" "}
        <span className="text-added">
          +{diff.stat.insertions}
        </span>{" "}
        <span className="text-removed">
          -{diff.stat.deletions}
        </span>
      </p>
      {files.map((file) => (
        <details
          key={file.path}
          open={file.lines.length <= FOLD_ABOVE_LINES}
          className="overflow-hidden rounded-lg border"
        >
          <summary className="flex cursor-pointer items-center gap-3 bg-muted/60 px-3 py-1.5 text-sm select-none">
            <span className="truncate font-mono">{file.path}</span>
            <span className="ml-auto shrink-0 text-xs">
              <span className="text-added">
                +{file.additions}
              </span>{" "}
              <span className="text-removed">
                -{file.deletions}
              </span>
            </span>
          </summary>
          {/* Size and family of the text come from the settings. */}
          <div className="overflow-x-auto font-[family-name:var(--diff-font-family)] text-[length:var(--diff-font-size)] leading-[1.6]">
            <div className={cn(!diffWrap && "min-w-max")}>
              {file.lines.map((line, index) => (
                <div key={index} className={cn("flex", LINE_STYLE[line.kind])}>
                  {diffLineNumbers && (
                    <>
                      <span className="w-[4em] shrink-0 pr-2 text-right text-muted-foreground/70 select-none">
                        {line.oldNumber}
                      </span>
                      <span className="w-[4em] shrink-0 pr-2 text-right text-muted-foreground/70 select-none">
                        {line.newNumber}
                      </span>
                    </>
                  )}
                  <span className="w-[1.5em] shrink-0 pl-1 select-none">
                    {LINE_MARK[line.kind]}
                  </span>
                  <span
                    className={cn(
                      "pr-4",
                      diffWrap ? "break-all whitespace-pre-wrap" : "whitespace-pre",
                    )}
                  >
                    {line.text}
                  </span>
                </div>
              ))}
            </div>
          </div>
        </details>
      ))}
    </div>
  );
}
