import { expect, test } from "vitest";

import {
  DEFAULTS,
  activeTheme,
  parseSettings,
  resolveKind,
  resolveLocale,
} from "./settings";
import { THEMES, isColor, readableOn, themeVariables } from "./themes";

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
  expect(resolveKind("system", true)).toBe("dark");
  expect(resolveKind("system", false)).toBe("light");
  expect(resolveKind("light", true)).toBe("light");
  expect(resolveKind("dark", false)).toBe("dark");
});

test("stored settings are checked before they are used", () => {
  expect(parseSettings(null)).toEqual(DEFAULTS);
  expect(parseSettings("not json")).toEqual(DEFAULTS);
  expect(parseSettings("[1]")).toEqual(DEFAULTS);
  // What the first version stored still reads.
  expect(parseSettings('{"theme":"dark","language":"ja"}')).toEqual({
    ...DEFAULTS,
    theme: "dark",
    language: "ja",
  });
  expect(
    parseSettings(
      JSON.stringify({
        theme: "pink",
        language: 5,
        textSize: "huge",
        diffTextSize: 12.5,
        diffFontFamily: 3,
        diffWrap: "yes",
        watch: 0,
      }),
    ),
  ).toEqual(DEFAULTS);
  expect(
    parseSettings(
      JSON.stringify({
        textSize: "large",
        diffTextSize: 14,
        diffFontFamily: "JetBrains Mono",
        diffWrap: true,
        diffLineNumbers: false,
        watch: false,
        verifyOnPublish: false,
      }),
    ),
  ).toEqual({
    ...DEFAULTS,
    textSize: "large",
    diffTextSize: 14,
    diffFontFamily: "JetBrains Mono",
    diffWrap: true,
    diffLineNumbers: false,
    watch: false,
    verifyOnPublish: false,
  });
});

test("a theme is only accepted for the kind it has", () => {
  const settings = parseSettings(
    JSON.stringify({ lightTheme: "nord", darkTheme: "dracula" }),
  );
  // Nord is dark, so it cannot be the light theme.
  expect(settings.lightTheme).toBe("tsugi-light");
  expect(settings.darkTheme).toBe("dracula");
  expect(parseSettings('{"darkTheme":"no-such-theme"}').darkTheme).toBe(
    "tsugi-dark",
  );
});

test("custom colours are kept only if they are colours of a known theme", () => {
  const settings = parseSettings(
    JSON.stringify({
      customColors: {
        nord: { accent: "#ff0000", background: "red", unknown: "#000000" },
        "no-such-theme": { accent: "#ff0000" },
        dracula: { accent: 12 },
      },
    }),
  );
  expect(settings.customColors).toEqual({ nord: { accent: "#ff0000" } });
});

test("the active theme follows the mode, and custom colours apply to it", () => {
  const settings = {
    ...DEFAULTS,
    lightTheme: "solarized-light",
    darkTheme: "nord",
    customColors: { nord: { accent: "#ffee00" } },
  };
  expect(activeTheme(settings, false).theme.id).toBe("solarized-light");
  const dark = activeTheme(settings, true);
  expect(dark.theme.id).toBe("nord");
  expect(dark.colors.accent).toBe("#ffee00");
  // Text on a changed accent stays readable.
  expect(dark.colors.accentForeground).toBe("#000000");
  expect(dark.colors.background).toBe("#2e3440");

  expect(activeTheme({ ...settings, theme: "light" }, true).theme.id).toBe(
    "solarized-light",
  );
});

test("every theme is complete and readable", () => {
  const ids = new Set(THEMES.map((theme) => theme.id));
  expect(ids.size).toBe(THEMES.length);
  for (const theme of THEMES) {
    for (const color of Object.values(theme.colors)) {
      expect(isColor(color), `${theme.id}: ${color}`).toBe(true);
    }
    // The kind says which way the contrast goes.
    const textOnBackground = readableOn(theme.colors.background);
    expect(textOnBackground, theme.id).toBe(
      theme.kind === "dark" ? "#ffffff" : "#000000",
    );
    expect(Object.keys(themeVariables(theme.colors))).toContain("--background");
  }
});

test("readable text is black on light colours and white on dark ones", () => {
  expect(readableOn("#ffffff")).toBe("#000000");
  expect(readableOn("#fabd2f")).toBe("#000000");
  expect(readableOn("#000000")).toBe("#ffffff");
  expect(readableOn("#268bd2")).toBe("#000000");
  expect(readableOn("#5e3fbe")).toBe("#ffffff");
});
