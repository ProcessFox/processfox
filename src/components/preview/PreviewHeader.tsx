import { X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { iconForFile } from "@/lib/fileIcons";

export type PreviewStatus =
  | { kind: "idle" }
  | { kind: "saving" }
  | { kind: "saved" }
  | { kind: "conflict" }
  | { kind: "error"; message: string };

type Props = {
  fileName: string;
  status?: PreviewStatus;
  onClose: () => void;
};

export function PreviewHeader({ fileName, status, onClose }: Props) {
  const { t } = useTranslation();
  const Icon = iconForFile(fileName);
  return (
    <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border bg-surface px-3 py-2">
      <div className="flex min-w-0 items-center gap-2">
        <Icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        <span className="truncate text-sm font-medium">{fileName}</span>
      </div>
      <div className="flex items-center gap-2">
        {status && <StatusLabel status={status} />}
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7"
          onClick={onClose}
          title={t("preview.closePreview")}
        >
          <X className="h-3.5 w-3.5" />
        </Button>
      </div>
    </div>
  );
}

function StatusLabel({ status }: { status: PreviewStatus }) {
  const { t } = useTranslation();
  if (status.kind === "idle") return null;
  if (status.kind === "saving") {
    return <span className="text-xs text-muted-foreground">{t("preview.saving")}</span>;
  }
  if (status.kind === "saved") {
    return <span className="text-xs text-muted-foreground">{t("preview.saved")}</span>;
  }
  if (status.kind === "conflict") {
    return (
      <span className="text-xs text-amber-600 dark:text-amber-400">
        {t("preview.conflict")}
      </span>
    );
  }
  return (
    <span className="text-xs text-destructive" title={status.message}>
      {t("preview.error")}
    </span>
  );
}
