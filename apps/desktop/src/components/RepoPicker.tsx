import { FolderOpenIcon, SettingsIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

import { ErrorNote } from "@/components/ErrorNote";
import { Button } from "@/components/ui/button";
import { baseName } from "@/lib/format";

type Props = {
  /** Repositories that were open before, most recent first. */
  recent: string[];
  /** Why the last attempt to open a repository failed, if it did. */
  error: unknown;
  onChoose: () => void;
  onOpenPath: (path: string) => void;
  onSettings: () => void;
};

/** The screen shown while no repository is open. */
export function RepoPicker({
  recent,
  error,
  onChoose,
  onOpenPath,
  onSettings,
}: Props) {
  const { t } = useTranslation();
  return (
    <main className="relative flex h-screen flex-col items-center justify-center gap-8 p-8">
      <Button
        variant="ghost"
        size="icon-sm"
        className="absolute top-3 right-3"
        aria-label={t("header.settings")}
        onClick={onSettings}
      >
        <SettingsIcon />
      </Button>
      <div className="text-center">
        <h1 className="text-3xl font-semibold tracking-tight">tsugi desktop</h1>
        <p className="mt-2 text-muted-foreground">{t("picker.tagline")}</p>
      </div>

      <Button size="lg" onClick={onChoose}>
        <FolderOpenIcon />
        {t("picker.open")}
      </Button>
      {error !== null && (
        <ErrorNote error={error} className="max-w-lg text-center" />
      )}

      {recent.length > 0 && (
        <section className="w-full max-w-lg">
          <h2 className="mb-2 text-xs font-medium text-muted-foreground uppercase">
            {t("picker.recent")}
          </h2>
          <ul className="divide-y rounded-lg border">
            {recent.map((path) => (
              <li key={path}>
                <button
                  type="button"
                  className="flex w-full flex-col px-3 py-2 text-left hover:bg-muted"
                  onClick={() => onOpenPath(path)}
                >
                  <span className="text-sm font-medium">{baseName(path)}</span>
                  <span className="truncate text-xs text-muted-foreground">
                    {path}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </main>
  );
}
