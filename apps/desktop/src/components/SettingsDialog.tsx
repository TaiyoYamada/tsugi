import { useQuery } from "@tanstack/react-query";
import { getVersion } from "@tauri-apps/api/app";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";

import { ConfirmDialog } from "@/components/dialogs";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  DIFF_TEXT_SIZES,
  LANGUAGES,
  type Language,
  MODES,
  type Mode,
  TEXT_SIZES,
  type TextSize,
} from "@/lib/settings";
import {
  resetSettings,
  updateSettings,
  useActiveTheme,
  useSettings,
} from "@/lib/settings-store";
import {
  COLOR_KEYS,
  type ColorKey,
  THEMES,
  type ThemeKind,
  withCustomColors,
} from "@/lib/themes";
import { cn } from "@/lib/utils";

const SECTIONS = [
  "general",
  "appearance",
  "diff",
  "publishing",
  "about",
] as const;
type Section = (typeof SECTIONS)[number];

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function SettingsDialog({ open, onOpenChange }: Props) {
  const { t } = useTranslation();
  const [section, setSection] = useState<Section>("general");
  const [restoring, setRestoring] = useState(false);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex h-[38rem] max-h-[90vh] gap-0 overflow-hidden p-0 sm:max-w-3xl">
        <nav className="flex w-48 shrink-0 flex-col gap-0.5 border-r bg-muted/40 p-3">
          <DialogTitle className="px-2 pt-1 pb-3">
            {t("settings.title")}
          </DialogTitle>
          <DialogDescription className="sr-only">
            {t("settings.title")}
          </DialogDescription>
          {SECTIONS.map((name) => (
            <button
              key={name}
              type="button"
              aria-current={name === section}
              onClick={() => setSection(name)}
              className={cn(
                "rounded-md px-2 py-1.5 text-left text-sm hover:bg-muted",
                name === section && "bg-muted font-medium",
              )}
            >
              {t(`settings.${name}`)}
            </button>
          ))}
          <Button
            variant="ghost"
            size="sm"
            className="mt-auto justify-start text-muted-foreground"
            onClick={() => setRestoring(true)}
          >
            {t("settings.restore")}
          </Button>
        </nav>

        <div className="flex min-w-0 flex-1 flex-col gap-6 overflow-y-auto p-6 pr-12">
          {section === "general" && <General />}
          {section === "appearance" && <Appearance />}
          {section === "diff" && <DiffSettings />}
          {section === "publishing" && <Publishing />}
          {section === "about" && <About />}
        </div>
      </DialogContent>

      <ConfirmDialog
        open={restoring}
        onOpenChange={setRestoring}
        title={t("settings.restoreTitle")}
        description={t("settings.restoreDescription")}
        confirmLabel={t("settings.restoreConfirm")}
        destructive
        onConfirm={resetSettings}
      />
    </Dialog>
  );
}

function General() {
  const { t } = useTranslation();
  const settings = useSettings();
  // Languages are named in their own language, whatever the app shows.
  const languageNames: Record<Language, string> = {
    system: t("settings.system"),
    en: "English",
    ja: "日本語",
  };
  return (
    <>
      <Setting title={t("settings.language")} hint={t("settings.languageHint")}>
        <Choice
          label={t("settings.language")}
          options={LANGUAGES}
          names={languageNames}
          value={settings.language}
          onChange={(language) => updateSettings({ language })}
        />
      </Setting>
      <Toggle
        title={t("settings.watch")}
        hint={t("settings.watchHint")}
        checked={settings.watch}
        onChange={(watch) => updateSettings({ watch })}
      />
    </>
  );
}

function Appearance() {
  const { t } = useTranslation();
  const settings = useSettings();
  const active = useActiveTheme();
  const modeNames: Record<Mode, string> = {
    system: t("settings.system"),
    light: t("settings.light"),
    dark: t("settings.dark"),
  };
  const textSizeNames: Record<TextSize, string> = {
    small: t("settings.textSizeSmall"),
    medium: t("settings.textSizeMedium"),
    large: t("settings.textSizeLarge"),
    larger: t("settings.textSizeLarger"),
  };
  const colorNames: Record<ColorKey, string> = {
    background: t("settings.colorBackground"),
    foreground: t("settings.colorForeground"),
    mutedForeground: t("settings.colorMutedForeground"),
    surface: t("settings.colorSurface"),
    border: t("settings.colorBorder"),
    accent: t("settings.colorAccent"),
    accentForeground: t("settings.colorAccentForeground"),
    added: t("settings.colorAdded"),
    removed: t("settings.colorRemoved"),
  };
  const custom = settings.customColors[active.theme.id] ?? {};

  function setColor(key: ColorKey, color: string | null) {
    const colors = { ...custom };
    if (color === null) delete colors[key];
    else colors[key] = color;
    updateSettings({
      customColors: { ...settings.customColors, [active.theme.id]: colors },
    });
  }

  return (
    <>
      <Setting title={t("settings.mode")} hint={t("settings.modeHint")}>
        <Choice
          label={t("settings.mode")}
          options={MODES}
          names={modeNames}
          value={settings.theme}
          onChange={(theme) => updateSettings({ theme })}
        />
      </Setting>
      <Setting
        title={t("settings.lightTheme")}
        hint={t("settings.lightThemeHint")}
      >
        <ThemeGrid kind="light" />
      </Setting>
      <Setting
        title={t("settings.darkTheme")}
        hint={t("settings.darkThemeHint")}
      >
        <ThemeGrid kind="dark" />
      </Setting>

      <Setting
        title={t("settings.colors", { theme: active.theme.name })}
        hint={t("settings.colorsHint")}
      >
        <ul className="divide-y rounded-lg border">
          {COLOR_KEYS.map((key) => (
            <li key={key} className="flex items-center gap-3 px-3 py-1.5">
              <input
                type="color"
                aria-label={colorNames[key]}
                value={active.colors[key]}
                onChange={(event) => setColor(key, event.target.value)}
                className="size-7 shrink-0 cursor-pointer rounded border bg-transparent p-0.5"
              />
              <span className="min-w-0 flex-1 truncate text-sm">
                {colorNames[key]}
              </span>
              <span className="font-mono text-xs text-muted-foreground">
                {active.colors[key]}
              </span>
              <Button
                variant="ghost"
                size="xs"
                className={cn(custom[key] === undefined && "invisible")}
                onClick={() => setColor(key, null)}
              >
                {t("settings.resetColor")}
              </Button>
            </li>
          ))}
        </ul>
        <Button
          variant="outline"
          size="sm"
          className="self-start"
          disabled={Object.keys(custom).length === 0}
          onClick={() =>
            updateSettings({
              customColors: { ...settings.customColors, [active.theme.id]: {} },
            })
          }
        >
          {t("settings.resetColors")}
        </Button>
      </Setting>

      <Setting title={t("settings.textSize")} hint={t("settings.textSizeHint")}>
        <Choice
          label={t("settings.textSize")}
          options={TEXT_SIZES}
          names={textSizeNames}
          value={settings.textSize}
          onChange={(textSize) => updateSettings({ textSize })}
        />
      </Setting>
    </>
  );
}

/** The themes of one kind, each drawn in its own colours. */
function ThemeGrid({ kind }: { kind: ThemeKind }) {
  const { t } = useTranslation();
  const settings = useSettings();
  const active = useActiveTheme();
  const chosen = kind === "light" ? settings.lightTheme : settings.darkTheme;

  function pick(id: string) {
    updateSettings({
      ...(kind === "light" ? { lightTheme: id } : { darkTheme: id }),
      // Picking a theme of the other kind while the kind is fixed would
      // change nothing on screen; switch to the kind of the theme.
      ...(settings.theme === "system" ? {} : { theme: kind }),
    });
  }

  return (
    <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
      {THEMES.filter((theme) => theme.kind === kind).map((theme) => {
        const colors = withCustomColors(theme, settings.customColors[theme.id]);
        const selected = theme.id === chosen;
        return (
          <button
            key={theme.id}
            type="button"
            aria-pressed={selected}
            onClick={() => pick(theme.id)}
            style={{
              background: colors.background,
              color: colors.foreground,
              borderColor: selected ? colors.accent : colors.border,
            }}
            className="flex flex-col gap-2 rounded-lg border-2 p-2.5 text-left"
          >
            <span className="flex items-baseline gap-2">
              <span className="truncate text-sm font-medium">{theme.name}</span>
              {theme.id === active.theme.id && (
                <span
                  className="shrink-0 text-xs"
                  style={{ color: colors.mutedForeground }}
                >
                  {t("settings.inUse")}
                </span>
              )}
            </span>
            <span className="flex gap-1.5">
              {[colors.accent, colors.added, colors.removed, colors.surface].map(
                (color, index) => (
                  <span
                    key={index}
                    className="size-3.5 rounded-full border"
                    style={{ background: color, borderColor: colors.border }}
                  />
                ),
              )}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function DiffSettings() {
  const { t } = useTranslation();
  const settings = useSettings();
  const sizeNames = Object.fromEntries(
    DIFF_TEXT_SIZES.map((size) => [size, String(size)]),
  );
  return (
    <>
      <Setting
        title={t("settings.diffTextSize")}
        hint={t("settings.diffTextSizeHint")}
      >
        <Choice
          label={t("settings.diffTextSize")}
          options={DIFF_TEXT_SIZES}
          names={sizeNames}
          value={settings.diffTextSize}
          onChange={(diffTextSize) => updateSettings({ diffTextSize })}
        />
      </Setting>
      <Setting
        title={t("settings.diffFontFamily")}
        hint={t("settings.diffFontFamilyHint")}
      >
        <Input
          aria-label={t("settings.diffFontFamily")}
          value={settings.diffFontFamily}
          placeholder="ui-monospace"
          spellCheck={false}
          onChange={(event) =>
            updateSettings({ diffFontFamily: event.target.value })
          }
        />
        <p className="rounded-md border px-3 py-2 font-[family-name:var(--diff-font-family)] text-[length:var(--diff-font-size)] leading-[1.6]">
          {"fn main() { println!(\"0O 1lI\"); }"}
        </p>
      </Setting>
      <Toggle
        title={t("settings.diffWrap")}
        hint={t("settings.diffWrapHint")}
        checked={settings.diffWrap}
        onChange={(diffWrap) => updateSettings({ diffWrap })}
      />
      <Toggle
        title={t("settings.diffLineNumbers")}
        hint={t("settings.diffLineNumbersHint")}
        checked={settings.diffLineNumbers}
        onChange={(diffLineNumbers) => updateSettings({ diffLineNumbers })}
      />
    </>
  );
}

function Publishing() {
  const { t } = useTranslation();
  const settings = useSettings();
  return (
    <Toggle
      title={t("settings.verifyOnPublish")}
      hint={t("settings.verifyOnPublishHint")}
      checked={settings.verifyOnPublish}
      onChange={(verifyOnPublish) => updateSettings({ verifyOnPublish })}
    />
  );
}

function About() {
  const { t } = useTranslation();
  const version = useQuery({ queryKey: ["version"], queryFn: getVersion });
  const facts: [string, string][] = [
    [t("settings.version"), version.data ?? ""],
    [t("settings.license"), "Apache-2.0"],
    [t("settings.source"), "https://github.com/TaiyoYamada/tsugi"],
  ];
  return (
    <>
      <div>
        <h3 className="text-lg font-semibold">tsugi</h3>
        <p className="text-sm text-muted-foreground">{t("picker.tagline")}</p>
      </div>
      <dl className="grid grid-cols-[7rem_1fr] gap-x-4 gap-y-1.5 text-sm">
        {facts.map(([name, value]) => (
          <div key={name} className="contents">
            <dt className="text-muted-foreground">{name}</dt>
            <dd className="min-w-0 break-all select-text">{value}</dd>
          </div>
        ))}
      </dl>
      <Setting title={t("settings.data")} hint={t("settings.dataHint")} />
    </>
  );
}

/** One setting: what it is called, what it does, and its control. */
function Setting({
  title,
  hint,
  children,
}: {
  title: string;
  hint: string;
  children?: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-2">
      <div>
        <h3 className="text-sm font-medium">{title}</h3>
        <p className="text-xs text-muted-foreground">{hint}</p>
      </div>
      {children}
    </section>
  );
}

/** A setting that is on or off. */
function Toggle({
  title,
  hint,
  checked,
  onChange,
}: {
  title: string;
  hint: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-3">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="mt-0.5 size-4 shrink-0 accent-primary"
      />
      <span>
        <span className="block text-sm font-medium">{title}</span>
        <span className="block text-xs text-muted-foreground">{hint}</span>
      </span>
    </label>
  );
}

type ChoiceProps<T extends string | number> = {
  label: string;
  options: readonly T[];
  names: Record<T, string>;
  value: T;
  onChange: (value: T) => void;
};

/** A row of buttons of which exactly one is selected. */
function Choice<T extends string | number>({
  label,
  options,
  names,
  value,
  onChange,
}: ChoiceProps<T>) {
  return (
    <div role="radiogroup" aria-label={label} className="flex gap-1.5">
      {options.map((option) => (
        <Button
          key={option}
          role="radio"
          aria-checked={option === value}
          variant={option === value ? "default" : "outline"}
          size="sm"
          className="flex-1"
          onClick={() => onChange(option)}
        >
          {names[option]}
        </Button>
      ))}
    </div>
  );
}
