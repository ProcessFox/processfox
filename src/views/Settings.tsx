import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import i18n from "i18next";

import { CloudApisTab } from "@/components/settings/CloudApisTab";
import { ModelsTab } from "@/components/settings/ModelsTab";
import { useTheme, type Theme } from "@/components/theme-provider";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { settingsApi } from "@/lib/tauri";
import { SUPPORTED_LANGUAGES } from "@/lib/i18n";
import type { Settings } from "@/types/settings";

type Props = {
  open: boolean;
  defaultTab?: "models" | "cloud" | "appearance" | "about";
  onClose: () => void;
  onSettingsChange?: (s: Settings) => void;
};

export function SettingsDialog({
  open,
  defaultTab = "cloud",
  onClose,
  onSettingsChange,
}: Props) {
  const { t } = useTranslation();
  const { theme, setTheme } = useTheme();
  const [settings, setSettings] = useState<Settings | null>(null);

  const THEME_OPTIONS: { value: Theme; label: string }[] = [
    { value: "system", label: t("settings.themeSystem") },
    { value: "light", label: t("settings.themeLight") },
    { value: "dark", label: t("settings.themeDark") },
  ];

  useEffect(() => {
    if (!open) return;
    settingsApi.get().then(setSettings).catch(console.error);
  }, [open]);

  function handleSettingsChange(s: Settings) {
    setSettings(s);
    onSettingsChange?.(s);
  }

  async function handleLocaleChange(locale: string) {
    const value = locale === "system" ? null : locale;
    const next = await settingsApi.setLocale(value);
    handleSettingsChange(next);
    i18n.changeLanguage(value ?? undefined);
    if (!value) {
      localStorage.removeItem("processfox-locale");
    }
  }

  const currentLocale = settings?.locale ?? "system";

  return (
    <Dialog open={open} onOpenChange={(v) => !v && onClose()}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-[720px]">
        <DialogHeader>
          <DialogTitle>{t("settings.title")}</DialogTitle>
        </DialogHeader>

        <Tabs defaultValue={defaultTab}>
          <TabsList className="w-full justify-start">
            <TabsTrigger value="models">{t("settings.models")}</TabsTrigger>
            <TabsTrigger value="cloud">{t("settings.cloudApis")}</TabsTrigger>
            <TabsTrigger value="appearance">{t("settings.appearance")}</TabsTrigger>
            <TabsTrigger value="about">{t("settings.about")}</TabsTrigger>
          </TabsList>

          <TabsContent value="models">
            <ModelsTab
              settings={settings}
              onSettingsChange={handleSettingsChange}
            />
          </TabsContent>

          <TabsContent value="cloud">
            <CloudApisTab
              settings={settings}
              onSettingsChange={handleSettingsChange}
            />
          </TabsContent>

          <TabsContent value="appearance" className="py-4">
            <div className="flex flex-col gap-5">
              <div className="flex flex-col gap-3">
                <div className="text-sm font-medium">{t("settings.language")}</div>
                <div className="flex flex-wrap gap-2">
                  <button
                    onClick={() => handleLocaleChange("system")}
                    className={`rounded-md border px-3 py-1.5 text-xs transition-colors ${
                      currentLocale === "system"
                        ? "border-primary bg-primary/10 text-foreground"
                        : "border-border bg-background text-muted-foreground hover:bg-accent"
                    }`}
                  >
                    {t("settings.languageSystem")}
                  </button>
                  {SUPPORTED_LANGUAGES.map((lang) => (
                    <button
                      key={lang.code}
                      onClick={() => handleLocaleChange(lang.code)}
                      className={`rounded-md border px-3 py-1.5 text-xs transition-colors ${
                        currentLocale === lang.code
                          ? "border-primary bg-primary/10 text-foreground"
                          : "border-border bg-background text-muted-foreground hover:bg-accent"
                      }`}
                    >
                      {lang.label}
                    </button>
                  ))}
                </div>
              </div>

              <div className="flex flex-col gap-3">
                <div className="text-sm font-medium">{t("settings.theme")}</div>
                <div className="flex gap-2">
                  {THEME_OPTIONS.map((opt) => (
                    <button
                      key={opt.value}
                      onClick={() => setTheme(opt.value)}
                      className={`rounded-md border px-3 py-1.5 text-xs transition-colors ${
                        theme === opt.value
                          ? "border-primary bg-primary/10 text-foreground"
                          : "border-border bg-background text-muted-foreground hover:bg-accent"
                      }`}
                    >
                      {opt.label}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          </TabsContent>

          <TabsContent value="about" className="py-4">
            <div className="flex flex-col gap-1 text-xs">
              <div className="text-sm font-medium">ProcessFox</div>
              <div className="text-muted-foreground">
                {t("settings.version")}
              </div>
              <div className="text-muted-foreground">
                {t("settings.tagline")}
              </div>
            </div>
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  );
}
