// The settings of the app and the rules that turn them into what is shown.
// `settings-store.ts` keeps and applies them.

import type { Locale } from "@/i18n";
import {
  COLOR_KEYS,
  type ColorTheme,
  DEFAULT_THEME,
  type ThemeColors,
  type ThemeKind,
  findTheme,
  isColor,
  withCustomColors,
} from "@/lib/themes";

export const MODES = ["system", "light", "dark"] as const;
export type Mode = (typeof MODES)[number];

export const LANGUAGES = ["system", "en", "ja"] as const;
export type Language = (typeof LANGUAGES)[number];

/** Size of the text of the interface, relative to the default. */
export const TEXT_SIZES = ["small", "medium", "large", "larger"] as const;
export type TextSize = (typeof TEXT_SIZES)[number];
export const TEXT_SIZE_PERCENT: Record<TextSize, number> = {
  small: 87.5,
  medium: 100,
  large: 112.5,
  larger: 125,
};

/** Size of the text of diffs, in pixels. */
export const DIFF_TEXT_SIZES = [11, 12, 13, 14, 16, 18] as const;

export type Settings = {
  language: Language;
  /** Light or dark. Stored as `theme` since the first version. */
  theme: Mode;
  /** Id of the colour theme used while the app is light, and while dark. */
  lightTheme: string;
  darkTheme: string;
  /** Colours the user changed, by id of the theme they belong to. */
  customColors: Record<string, Partial<ThemeColors>>;
  textSize: TextSize;
  diffTextSize: number;
  /** A CSS font family for diffs; empty for the default. */
  diffFontFamily: string;
  diffWrap: boolean;
  diffLineNumbers: boolean;
  /** Snapshot the open repositories as their files change. */
  watch: boolean;
  /** Whether "run the verification commands" starts ticked. */
  verifyOnPublish: boolean;
};

export const DEFAULTS: Settings = {
  language: "system",
  theme: "system",
  lightTheme: DEFAULT_THEME.light,
  darkTheme: DEFAULT_THEME.dark,
  customColors: {},
  textSize: "medium",
  diffTextSize: 12,
  diffFontFamily: "",
  diffWrap: false,
  diffLineNumbers: true,
  watch: true,
  verifyOnPublish: true,
};

function isOneOf<T extends string | number>(
  values: readonly T[],
  value: unknown,
): value is T {
  return values.some((candidate) => candidate === value);
}

/** The entries of a stored JSON object; none if it is anything else. */
function entries(value: unknown): Map<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? new Map(Object.entries(value))
    : new Map();
}

function themeOfKind(id: unknown, kind: ThemeKind): string {
  return typeof id === "string" && findTheme(id)?.kind === kind
    ? id
    : DEFAULT_THEME[kind];
}

function customColors(value: unknown): Settings["customColors"] {
  const result: Settings["customColors"] = {};
  for (const [id, stored] of entries(value)) {
    if (!findTheme(id)) continue;
    const colors: Partial<ThemeColors> = {};
    for (const key of COLOR_KEYS) {
      const color = entries(stored).get(key);
      if (isColor(color)) colors[key] = color;
    }
    if (Object.keys(colors).length > 0) result[id] = colors;
  }
  return result;
}

/** Reads stored settings; anything that is missing or invalid is the default. */
export function parseSettings(stored: string | null): Settings {
  let parsed: unknown;
  try {
    parsed = JSON.parse(stored ?? "{}");
  } catch {
    return DEFAULTS;
  }
  const value = entries(parsed);
  const choice = <T extends string | number>(
    key: keyof Settings,
    values: readonly T[],
    fallback: T,
  ): T => {
    const candidate = value.get(key);
    return isOneOf(values, candidate) ? candidate : fallback;
  };
  const flag = (key: keyof Settings, fallback: boolean): boolean => {
    const candidate = value.get(key);
    return typeof candidate === "boolean" ? candidate : fallback;
  };
  const family = value.get("diffFontFamily");
  return {
    language: choice("language", LANGUAGES, DEFAULTS.language),
    theme: choice("theme", MODES, DEFAULTS.theme),
    lightTheme: themeOfKind(value.get("lightTheme"), "light"),
    darkTheme: themeOfKind(value.get("darkTheme"), "dark"),
    customColors: customColors(value.get("customColors")),
    textSize: choice("textSize", TEXT_SIZES, DEFAULTS.textSize),
    diffTextSize: choice("diffTextSize", DIFF_TEXT_SIZES, DEFAULTS.diffTextSize),
    diffFontFamily: typeof family === "string" ? family : DEFAULTS.diffFontFamily,
    diffWrap: flag("diffWrap", DEFAULTS.diffWrap),
    diffLineNumbers: flag("diffLineNumbers", DEFAULTS.diffLineNumbers),
    watch: flag("watch", DEFAULTS.watch),
    verifyOnPublish: flag("verifyOnPublish", DEFAULTS.verifyOnPublish),
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

export function resolveKind(mode: Mode, systemIsDark: boolean): ThemeKind {
  if (mode === "system") return systemIsDark ? "dark" : "light";
  return mode;
}

export type ActiveTheme = {
  theme: ColorTheme;
  /** The colours of the theme with the user's changes applied. */
  colors: ThemeColors;
};

/** The colour theme in effect, given the settings and the system. */
export function activeTheme(
  settings: Settings,
  systemIsDark: boolean,
): ActiveTheme {
  const kind = resolveKind(settings.theme, systemIsDark);
  const id = kind === "dark" ? settings.darkTheme : settings.lightTheme;
  const theme = findTheme(id) ?? findTheme(DEFAULT_THEME[kind]);
  if (!theme) throw new Error(`the default ${kind} theme is missing`);
  return {
    theme,
    colors: withCustomColors(theme, settings.customColors[theme.id]),
  };
}
