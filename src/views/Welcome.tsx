import { Check, Cpu, FolderOpen, Sparkles } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { Agent } from "@/types/agent";
import type { InstalledModel } from "@/types/models";
import type { Settings } from "@/types/settings";

type Props = {
  open: boolean;
  settings: Settings | null;
  installedModels: InstalledModel[];
  hasApiKey: boolean | null;
  agents: Agent[];
  onOpenSettings: () => void;
  onCreateAgent: () => void;
  onFinish: () => void;
};

export function WelcomeDialog({
  open,
  settings,
  installedModels,
  hasApiKey,
  agents,
  onOpenSettings,
  onCreateAgent,
  onFinish,
}: Props) {
  const { t } = useTranslation();
  const modelReady = isModelReady(settings, installedModels, hasApiKey);
  const agentReady = agents.length > 0;
  const allReady = modelReady && agentReady;

  return (
    <Dialog open={open}>
      <DialogContent
        className="sm:max-w-[560px]"
        onPointerDownOutside={(e) => e.preventDefault()}
        onEscapeKeyDown={(e) => e.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="h-4 w-4" />
            {t("welcome.title")}
          </DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-4 py-2">
          <p className="text-sm text-muted-foreground">
            {t("welcome.intro")}
          </p>

          <Step
            number={1}
            done={modelReady}
            icon={Cpu}
            title={t("welcome.step1Title")}
            description={
              modelReady
                ? t("welcome.step1Done")
                : t("welcome.step1Todo")
            }
            actionLabel={modelReady ? t("welcome.step1ActionDone") : t("welcome.step1ActionTodo")}
            onAction={onOpenSettings}
          />

          <Step
            number={2}
            done={agentReady}
            icon={FolderOpen}
            title={t("welcome.step2Title")}
            description={
              agentReady
                ? t("welcome.step2Done", { count: agents.length })
                : t("welcome.step2Todo")
            }
            actionLabel={agentReady ? t("welcome.step2ActionDone") : t("welcome.step2ActionTodo")}
            onAction={onCreateAgent}
            disabled={!modelReady}
          />
        </div>

        <div className="flex justify-end pt-2">
          <Button onClick={onFinish} disabled={!allReady}>
            {t("welcome.finish")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function Step({
  number,
  done,
  icon: Icon,
  title,
  description,
  actionLabel,
  onAction,
  disabled,
}: {
  number: number;
  done: boolean;
  icon: typeof Cpu;
  title: string;
  description: string;
  actionLabel: string;
  onAction: () => void;
  disabled?: boolean;
}) {
  return (
    <div
      className={`flex items-start gap-3 rounded-md border p-3 ${
        done
          ? "border-emerald-500/30 bg-emerald-500/5"
          : "border-border bg-background"
      }`}
    >
      <div
        className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-xs font-medium ${
          done
            ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300"
            : "bg-muted text-muted-foreground"
        }`}
      >
        {done ? <Check className="h-3.5 w-3.5" /> : number}
      </div>
      <div className="flex-1">
        <div className="flex items-center gap-1.5 text-sm font-medium">
          <Icon className="h-3.5 w-3.5 opacity-70" />
          {title}
        </div>
        <div className="mt-0.5 text-xs text-muted-foreground">{description}</div>
      </div>
      <Button
        size="sm"
        variant={done ? "ghost" : "default"}
        onClick={onAction}
        disabled={disabled}
      >
        {actionLabel}
      </Button>
    </div>
  );
}

function isModelReady(
  settings: Settings | null,
  installedModels: InstalledModel[],
  hasApiKey: boolean | null,
): boolean {
  const provider = settings?.defaultProvider;
  if (!provider) return false;
  if (provider === "local") {
    const modelId = settings?.defaultModels?.[provider];
    if (!modelId) return false;
    return installedModels.some((m) => m.filename === modelId);
  }
  return hasApiKey === true;
}
