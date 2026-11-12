import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { LANGUAGES, type Language, THEMES, type Theme } from "@/lib/settings";
import { updateSettings, useSettings } from "@/lib/settings-store";

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function SettingsDialog({ open, onOpenChange }: Props) {
  const { t } = useTranslation();
  const settings = useSettings();

  // Languages are named in their own language, whatever the app shows.
  const languageNames: Record<Language, string> = {
    system: t("settings.system"),
    en: "English",
    ja: "日本語",
  };
  const themeNames: Record<Theme, string> = {
    system: t("settings.system"),
    light: t("settings.light"),
    dark: t("settings.dark"),
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("settings.title")}</DialogTitle>
        </DialogHeader>
        <Choice
          label={t("settings.language")}
          options={LANGUAGES}
          names={languageNames}
          value={settings.language}
          onChange={(language) => updateSettings({ language })}
        />
        <Choice
          label={t("settings.appearance")}
          options={THEMES}
          names={themeNames}
          value={settings.theme}
          onChange={(theme) => updateSettings({ theme })}
        />
      </DialogContent>
    </Dialog>
  );
}

type ChoiceProps<T extends string> = {
  label: string;
  options: readonly T[];
  names: Record<T, string>;
  value: T;
  onChange: (value: T) => void;
};

/** A row of buttons of which exactly one is selected. */
function Choice<T extends string>({
  label,
  options,
  names,
  value,
  onChange,
}: ChoiceProps<T>) {
  return (
    <div role="radiogroup" aria-label={label} className="flex flex-col gap-1.5">
      <p className="text-sm font-medium">{label}</p>
      <div className="flex gap-1.5">
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
    </div>
  );
}
