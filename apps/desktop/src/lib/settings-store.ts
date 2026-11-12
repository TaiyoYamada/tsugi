// Keeps the settings in localStorage and applies them to the document as
// soon as they change.

import { useSyncExternalStore } from "react";

import i18n from "@/i18n";
import {
  type ActiveTheme,
  DEFAULTS,
  type Settings,
  TEXT_SIZE_PERCENT,
  activeTheme,
  parseSettings,
  resolveLocale,
} from "@/lib/settings";
import { themeVariables } from "@/lib/themes";

const KEY = "tsugi.settings";
const DEFAULT_DIFF_FONT = "ui-monospace, SFMono-Regular, Menlo, monospace";

let current = parseSettings(localStorage.getItem(KEY));
const listeners = new Set<() => void>();
const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
let active: ActiveTheme = activeTheme(current, systemDark.matches);

function apply() {
  active = activeTheme(current, systemDark.matches);
  const root = document.documentElement;
  // `dark:` styles of the components follow the kind of the theme.
  root.classList.toggle("dark", active.theme.kind === "dark");
  for (const [name, value] of Object.entries(themeVariables(active.colors))) {
    root.style.setProperty(name, value);
  }
  root.style.fontSize = `${TEXT_SIZE_PERCENT[current.textSize]}%`;
  root.style.setProperty("--diff-font-size", `${current.diffTextSize}px`);
  root.style.setProperty(
    "--diff-font-family",
    current.diffFontFamily.trim() === ""
      ? DEFAULT_DIFF_FONT
      : `${current.diffFontFamily}, ${DEFAULT_DIFF_FONT}`,
  );

  const locale = resolveLocale(current.language, navigator.languages);
  root.lang = locale;
  void i18n.changeLanguage(locale);
}

function changed() {
  apply();
  for (const listener of listeners) listener();
}

// With `system`, the app follows the system while it runs.
systemDark.addEventListener("change", changed);
window.addEventListener("languagechange", changed);
apply();

export function updateSettings(change: Partial<Settings>) {
  current = { ...current, ...change };
  localStorage.setItem(KEY, JSON.stringify(current));
  changed();
}

export function resetSettings() {
  updateSettings(DEFAULTS);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function useSettings(): Settings {
  return useSyncExternalStore(subscribe, () => current);
}

/** The colour theme in effect right now. */
export function useActiveTheme(): ActiveTheme {
  return useSyncExternalStore(subscribe, () => active);
}
