// The colour themes of the app. A theme is nine colours; every colour the
// interface uses is one of them.
//
// Apart from the two tsugi themes, the palettes are those of the projects
// they are named after: Solarized, Gruvbox, Catppuccin, Nord, Dracula,
// Tokyo Night and One Dark. Where a palette has no colour for a role (a
// border, or secondary text that stays readable at small sizes), the value
// is derived from its neighbours.

export const COLOR_KEYS = [
  "background",
  "foreground",
  "mutedForeground",
  "surface",
  "border",
  "accent",
  "accentForeground",
  "added",
  "removed",
] as const;
export type ColorKey = (typeof COLOR_KEYS)[number];

/** Colours as `#rrggbb`. */
export type ThemeColors = Record<ColorKey, string>;

export const THEME_KINDS = ["light", "dark"] as const;
export type ThemeKind = (typeof THEME_KINDS)[number];

export type ColorTheme = {
  id: string;
  name: string;
  kind: ThemeKind;
  colors: ThemeColors;
};

export const DEFAULT_THEME: Record<ThemeKind, string> = {
  light: "tsugi-light",
  dark: "tsugi-dark",
};

export const THEMES: readonly ColorTheme[] = [
  {
    id: "tsugi-light",
    name: "tsugi Light",
    kind: "light",
    colors: {
      background: "#ffffff",
      foreground: "#0a0a0a",
      mutedForeground: "#737373",
      surface: "#f5f5f5",
      border: "#e5e5e5",
      accent: "#171717",
      accentForeground: "#fafafa",
      added: "#15803d",
      removed: "#dc2626",
    },
  },
  {
    id: "solarized-light",
    name: "Solarized Light",
    kind: "light",
    colors: {
      background: "#fdf6e3",
      foreground: "#586e75",
      mutedForeground: "#839496",
      surface: "#eee8d5",
      border: "#ddd6c1",
      accent: "#268bd2",
      accentForeground: "#fdf6e3",
      added: "#859900",
      removed: "#dc322f",
    },
  },
  {
    id: "gruvbox-light",
    name: "Gruvbox Light",
    kind: "light",
    colors: {
      background: "#fbf1c7",
      foreground: "#3c3836",
      mutedForeground: "#7c6f64",
      surface: "#ebdbb2",
      border: "#d5c4a1",
      accent: "#076678",
      accentForeground: "#fbf1c7",
      added: "#79740e",
      removed: "#9d0006",
    },
  },
  {
    id: "catppuccin-latte",
    name: "Catppuccin Latte",
    kind: "light",
    colors: {
      background: "#eff1f5",
      foreground: "#4c4f69",
      mutedForeground: "#6c6f85",
      surface: "#dce0e8",
      border: "#ccd0da",
      accent: "#8839ef",
      accentForeground: "#eff1f5",
      added: "#40a02b",
      removed: "#d20f39",
    },
  },
  {
    id: "tsugi-dark",
    name: "tsugi Dark",
    kind: "dark",
    colors: {
      background: "#0a0a0a",
      foreground: "#fafafa",
      mutedForeground: "#a1a1a1",
      surface: "#262626",
      border: "#2e2e2e",
      accent: "#e5e5e5",
      accentForeground: "#171717",
      added: "#4ade80",
      removed: "#f87171",
    },
  },
  {
    id: "solarized-dark",
    name: "Solarized Dark",
    kind: "dark",
    colors: {
      background: "#002b36",
      foreground: "#93a1a1",
      mutedForeground: "#657b83",
      surface: "#073642",
      border: "#124652",
      accent: "#268bd2",
      accentForeground: "#fdf6e3",
      added: "#859900",
      removed: "#dc322f",
    },
  },
  {
    id: "gruvbox-dark",
    name: "Gruvbox Dark",
    kind: "dark",
    colors: {
      background: "#282828",
      foreground: "#ebdbb2",
      mutedForeground: "#a89984",
      surface: "#3c3836",
      border: "#504945",
      accent: "#fabd2f",
      accentForeground: "#282828",
      added: "#b8bb26",
      removed: "#fb4934",
    },
  },
  {
    id: "catppuccin-mocha",
    name: "Catppuccin Mocha",
    kind: "dark",
    colors: {
      background: "#1e1e2e",
      foreground: "#cdd6f4",
      mutedForeground: "#a6adc8",
      surface: "#313244",
      border: "#45475a",
      accent: "#cba6f7",
      accentForeground: "#1e1e2e",
      added: "#a6e3a1",
      removed: "#f38ba8",
    },
  },
  {
    id: "nord",
    name: "Nord",
    kind: "dark",
    colors: {
      background: "#2e3440",
      foreground: "#eceff4",
      mutedForeground: "#a3adbf",
      surface: "#3b4252",
      border: "#4c566a",
      accent: "#88c0d0",
      accentForeground: "#2e3440",
      added: "#a3be8c",
      removed: "#bf616a",
    },
  },
  {
    id: "dracula",
    name: "Dracula",
    kind: "dark",
    colors: {
      background: "#282a36",
      foreground: "#f8f8f2",
      mutedForeground: "#a4abc8",
      surface: "#343746",
      border: "#44475a",
      accent: "#bd93f9",
      accentForeground: "#282a36",
      added: "#50fa7b",
      removed: "#ff5555",
    },
  },
  {
    id: "tokyo-night",
    name: "Tokyo Night",
    kind: "dark",
    colors: {
      background: "#1a1b26",
      foreground: "#c0caf5",
      mutedForeground: "#a9b1d6",
      surface: "#292e42",
      border: "#3b4261",
      accent: "#7aa2f7",
      accentForeground: "#1a1b26",
      added: "#9ece6a",
      removed: "#f7768e",
    },
  },
  {
    id: "one-dark",
    name: "One Dark",
    kind: "dark",
    colors: {
      background: "#282c34",
      foreground: "#abb2bf",
      mutedForeground: "#7f848e",
      surface: "#3e4451",
      border: "#3b4048",
      accent: "#61afef",
      accentForeground: "#282c34",
      added: "#98c379",
      removed: "#e06c75",
    },
  },
];

export function findTheme(id: string): ColorTheme | undefined {
  return THEMES.find((theme) => theme.id === id);
}

export function isColor(value: unknown): value is string {
  return typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value);
}

/** Black or white, whichever is easier to read on `color`. */
export function readableOn(color: string): string {
  const channel = (offset: number) => {
    const value = parseInt(color.slice(offset, offset + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  // Relative luminance, as WCAG defines it.
  const luminance = 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
  return luminance > 0.179 ? "#000000" : "#ffffff";
}

/**
 * The colours of a theme with the user's changes applied. An accent that
 * was changed gets a readable text colour unless that was changed too.
 */
export function withCustomColors(
  theme: ColorTheme,
  custom: Partial<ThemeColors> | undefined,
): ThemeColors {
  const colors = { ...theme.colors, ...custom };
  if (custom?.accent && !custom.accentForeground) {
    colors.accentForeground = readableOn(custom.accent);
  }
  return colors;
}

/** The CSS variables of index.css that the nine colours fill. */
export function themeVariables(colors: ThemeColors): Record<string, string> {
  return {
    "--background": colors.background,
    "--foreground": colors.foreground,
    "--card": colors.background,
    "--card-foreground": colors.foreground,
    "--popover": colors.background,
    "--popover-foreground": colors.foreground,
    "--primary": colors.accent,
    "--primary-foreground": colors.accentForeground,
    "--secondary": colors.surface,
    "--secondary-foreground": colors.foreground,
    "--muted": colors.surface,
    "--muted-foreground": colors.mutedForeground,
    "--accent": colors.surface,
    "--accent-foreground": colors.foreground,
    "--border": colors.border,
    "--input": colors.border,
    "--ring": colors.accent,
    "--destructive": colors.removed,
    "--added": colors.added,
    "--removed": colors.removed,
  };
}
