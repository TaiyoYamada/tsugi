import { expect, test } from "vitest";

import { isDark, parseSettings, resolveLocale } from "./settings";

test("the system language decides when the setting says so", () => {
  expect(resolveLocale("system", ["ja-JP", "en-US"])).toBe("ja");
  expect(resolveLocale("system", ["en-GB", "ja"])).toBe("en");
  // A language the app does not have is skipped.
  expect(resolveLocale("system", ["fr-FR", "ja-JP"])).toBe("ja");
  expect(resolveLocale("system", ["fr-FR"])).toBe("en");
  expect(resolveLocale("system", [])).toBe("en");
});

test("a chosen language wins over the system", () => {
  expect(resolveLocale("en", ["ja-JP"])).toBe("en");
  expect(resolveLocale("ja", ["en-US"])).toBe("ja");
});

test("the system appearance decides when the setting says so", () => {
  expect(isDark("system", true)).toBe(true);
  expect(isDark("system", false)).toBe(false);
  expect(isDark("light", true)).toBe(false);
  expect(isDark("dark", false)).toBe(true);
});

test("stored settings are checked before they are used", () => {
  expect(parseSettings(null)).toEqual({ theme: "system", language: "system" });
  expect(parseSettings('{"theme":"dark","language":"ja"}')).toEqual({
    theme: "dark",
    language: "ja",
  });
  expect(parseSettings('{"theme":"pink","language":5}')).toEqual({
    theme: "system",
    language: "system",
  });
  expect(parseSettings("not json")).toEqual({
    theme: "system",
    language: "system",
  });
  expect(parseSettings("[1]")).toEqual({ theme: "system", language: "system" });
});
