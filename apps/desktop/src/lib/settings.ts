// The settings of the app, language and appearance, and the rules that
// turn them into what is shown. `settings-store.ts` keeps and applies them.

import type { Locale } from "@/i18n";

export const THEMES = ["system", "light", "dark"] as const;
export type Theme = (typeof THEMES)[number];

export const LANGUAGES = ["system", "en", "ja"] as const;
export type Language = (typeof LANGUAGES)[number];

export type Settings = { theme: Theme; language: Language };

const DEFAULTS: Settings = { theme: "system", language: "system" };

function isOneOf<T extends string>(
  values: readonly T[],
  value: unknown,
): value is T {
  return values.some((candidate) => candidate === value);
}

/** Reads stored settings; anything that is missing or invalid is the default. */
export function parseSettings(stored: string | null): Settings {
  let value: unknown;
  try {
    value = JSON.parse(stored ?? "{}");
  } catch {
    return DEFAULTS;
  }
  if (typeof value !== "object" || value === null) return DEFAULTS;
  const theme = "theme" in value ? value.theme : undefined;
  const language = "language" in value ? value.language : undefined;
  return {
    theme: isOneOf(THEMES, theme) ? theme : DEFAULTS.theme,
    language: isOneOf(LANGUAGES, language) ? language : DEFAULTS.language,
  };
}

/**
 * The language to show. With `system`, the first language of the system
 * that the app has; English if it has none of them.
 */
export function resolveLocale(
  language: Language,
  systemLanguages: readonly string[],
): Locale {
  if (language !== "system") return language;
  for (const tag of systemLanguages) {
    const primary = tag.toLowerCase().split("-")[0];
    if (primary === "ja" || primary === "en") return primary;
  }
  return "en";
}

export function isDark(theme: Theme, systemIsDark: boolean): boolean {
  return theme === "system" ? systemIsDark : theme === "dark";
}
