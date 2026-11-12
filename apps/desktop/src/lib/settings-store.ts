// Keeps the settings in localStorage and applies them to the document as
// soon as they change.

import { useSyncExternalStore } from "react";

import i18n from "@/i18n";
import {
  type Settings,
  isDark,
  parseSettings,
  resolveLocale,
} from "@/lib/settings";

const KEY = "tsugi.settings";

let current = parseSettings(localStorage.getItem(KEY));
const listeners = new Set<() => void>();
const systemDark = window.matchMedia("(prefers-color-scheme: dark)");

function apply() {
  const locale = resolveLocale(current.language, navigator.languages);
  document.documentElement.classList.toggle(
    "dark",
    isDark(current.theme, systemDark.matches),
  );
  document.documentElement.lang = locale;
  void i18n.changeLanguage(locale);
}

// With `system`, the app follows the system while it runs.
systemDark.addEventListener("change", apply);
window.addEventListener("languagechange", apply);
apply();

export function updateSettings(change: Partial<Settings>) {
  current = { ...current, ...change };
  localStorage.setItem(KEY, JSON.stringify(current));
  apply();
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  systemDark.addEventListener("change", listener);
  return () => {
    listeners.delete(listener);
    systemDark.removeEventListener("change", listener);
  };
}

/** Whether the app is dark right now, whatever decided it. */
export function useDark(): boolean {
  return useSyncExternalStore(subscribe, () =>
    isDark(current.theme, systemDark.matches),
  );
}

export function useSettings(): Settings {
  return useSyncExternalStore(subscribe, () => current);
}
