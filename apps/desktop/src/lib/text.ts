// Texts that are put together from data: ages and errors.

import { useTranslation } from "react-i18next";

import { errorMessage, isCommandError } from "@/lib/api";
import { age } from "@/lib/format";

/** Returns a function that says how long ago a time was, as `3m ago`. */
export function useAgo(): (timestamp: number) => string {
  const { t } = useTranslation();
  return (timestamp) => {
    const result = age(timestamp);
    switch (result.unit) {
      case "now":
        return t("time.justNow");
      case "minutes":
        return t("time.minutesAgo", { count: result.count });
      case "hours":
        return t("time.hoursAgo", { count: result.count });
      case "days":
        return t("time.daysAgo", { count: result.count });
    }
  };
}

const ERROR_CODES = [
  "not_a_repository",
  "bare_repository",
  "not_initialized",
  "git_too_old",
  "workspace_not_found",
  "snapshot_not_found",
  "task_not_found",
  "name_taken",
  "invalid_name",
  "conflict",
  "verify_failed",
  "nothing_to_publish",
  "nothing_to_undo",
  "refused",
  "git_failed",
  "invalid_config",
  "io",
  "database",
  "json",
] as const;

export type ErrorText = {
  /** One sentence in the language of the app. */
  summary: string;
  /** What the Rust side reported, in English. */
  detail: string;
};

/** Returns a function that puts a failed command into words. */
export function useErrorText(): (error: unknown) => ErrorText {
  const { t } = useTranslation();
  return (error) => {
    const code = isCommandError(error)
      ? ERROR_CODES.find((known) => known === error.code)
      : undefined;
    return {
      summary: t(`errors.${code ?? "unknown"}`),
      detail: errorMessage(error),
    };
  };
}
