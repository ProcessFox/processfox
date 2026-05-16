import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowUp, BookOpen, FileStack, Plus, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { agentApi } from "@/lib/tauri";
import type { Agent, AttachmentKind } from "@/types/agent";

type Props = {
  disabled?: boolean;
  disabledReason?: string;
  onSend: (text: string) => void;
  /** Bump `token` to set the input value externally (e.g. starter chips).
   *  We watch the token rather than the text so the same prompt can be
   *  applied twice in a row. */
  prefill?: { text: string; token: number };
  /** The currently active agent. Used to read attachment state and bind
   *  attachment writes. When null, the attachment row is hidden. */
  agent?: Agent | null;
  /** Attachment kinds the agent's active skills accept. The button only
   *  appears for kinds in this list. Empty / undefined → no buttons. */
  acceptsAttachments?: string[];
  /** Called after a successful attachment write so the parent can refresh
   *  its agent state. */
  onAgentUpdated?: (agent: Agent) => void;
  /** Quiet status line shown below the input (template + model). Rendered
   *  inside the same surface band so it looks like part of the input area,
   *  not a separate footer panel. */
  footer?: { templateName: string | null; model: string | null };
};

const ATTACHMENT_EXTENSIONS: Record<string, string[]> = {
  template: ["docx"],
  context: ["md", "txt", "csv", "json", "docx", "pdf", "xlsx"],
};

export function ChatInput({
  disabled,
  disabledReason,
  onSend,
  prefill,
  agent,
  acceptsAttachments,
  onAgentUpdated,
  footer,
}: Props) {
  const { t } = useTranslation();
  const [value, setValue] = useState("");
  const ref = useRef<HTMLTextAreaElement>(null);
  const lastTokenRef = useRef<number | null>(null);

  useEffect(() => {
    if (!prefill) return;
    if (lastTokenRef.current === prefill.token) return;
    lastTokenRef.current = prefill.token;
    setValue(prefill.text);
    ref.current?.focus();
  }, [prefill]);

  function handleSend() {
    const trimmed = value.trim();
    if (!trimmed || disabled) return;
    onSend(trimmed);
    setValue("");
  }

  function handleKey(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      handleSend();
    }
  }

  // Render template left, context right, regardless of skill-list order so the
  // icon row stays stable when users toggle skills.
  const slotOrder: AttachmentKind[] = ["template", "context"];
  const acceptsSet = new Set(acceptsAttachments ?? []);
  const visibleSlots = slotOrder.filter(
    (k) => acceptsSet.has(k) && k in ATTACHMENT_EXTENSIONS,
  );

  return (
    <div className="border-t border-border bg-surface p-3">
      <div className="relative rounded-md border border-border bg-background focus-within:border-ring focus-within:ring-1 focus-within:ring-ring">
        <textarea
          ref={ref}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={handleKey}
          disabled={disabled}
          placeholder={
            disabled
              ? (disabledReason ?? t("chat.inputDisabled"))
              : t("chat.inputPlaceholder")
          }
          rows={3}
          className={`block w-full resize-none rounded-md bg-transparent px-3 py-2 pr-10 text-sm placeholder:text-muted-foreground focus:outline-none disabled:cursor-not-allowed disabled:opacity-60 ${
            visibleSlots.length > 0 ? "pb-9" : ""
          }`}
        />
        {visibleSlots.length > 0 && agent && (
          <div className="absolute bottom-1.5 left-1.5 flex items-center gap-1">
            {visibleSlots.map((kind) =>
              kind === "context" ? (
                <ContextAttachmentButton
                  key={kind}
                  agent={agent}
                  onAgentUpdated={onAgentUpdated}
                />
              ) : (
                <TemplateAttachmentButton
                  key={kind}
                  agent={agent}
                  onAgentUpdated={onAgentUpdated}
                />
              ),
            )}
          </div>
        )}
        <Button
          size="icon"
          className="absolute bottom-1.5 right-1.5 h-7 w-7"
          onClick={handleSend}
          disabled={disabled || value.trim().length === 0}
          title={t("chat.sendTitle")}
        >
          <ArrowUp className="h-3.5 w-3.5" />
        </Button>
      </div>
      {footer && (footer.templateName || footer.model) && (
        <div className="mt-1 flex items-center justify-end gap-2 text-[11px] text-muted-foreground">
          {footer.templateName && (
            <>
              <span
                className="min-w-0 truncate"
                title={`${t("chatInput.template")}: ${footer.templateName}`}
              >
                {t("chatInput.template")}: {footer.templateName}
              </span>
              {footer.model && <span className="opacity-40">·</span>}
            </>
          )}
          {footer.model && (
            <span className="shrink-0" title={footer.model!}>
              {footer.model}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

// Ghost variant on the left, distinct from the filled primary Send button on
// the right. Tone: warning (amber) when the slot is unset — gentle nudge that
// the agent still expects something; muted-foreground when satisfied.
function attachmentTone(filled: boolean): string {
  return filled
    ? "text-muted-foreground bg-muted hover:bg-accent/60"
    : "text-warning bg-warning/10 hover:bg-warning/20";
}

function TemplateAttachmentButton({
  agent,
  onAgentUpdated,
}: {
  agent: Agent;
  onAgentUpdated?: (agent: Agent) => void;
}) {
  const { t } = useTranslation();
  const extensions = ATTACHMENT_EXTENSIONS.template!;
  const label = t("chatInput.template");
  const dialogTitle = t("chatInput.chooseTemplate");
  const current = agent.attachments.templatePath ?? null;
  const hasAttachment = current !== null;
  const fileName = current ? (current.split(/[/\\]/).pop() ?? current) : null;

  async function handlePick() {
    try {
      const picked = await open({
        directory: false,
        multiple: false,
        title: dialogTitle,
        defaultPath: agent.folder ?? undefined,
        filters: [{ name: label, extensions }],
      });
      if (typeof picked !== "string") return;
      const updated = await agentApi.setAttachment(agent.id, "template", picked);
      onAgentUpdated?.(updated);
    } catch (e) {
      // Backend rejects paths outside the agent folder; show inline log only —
      // a toast would feel heavy for a single misclick.
      console.warn("attachment pick failed", e);
    }
  }

  const tooltipText = hasAttachment ? `${label}: ${fileName}` : dialogTitle;

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          onClick={handlePick}
          aria-label={tooltipText}
          className={`flex h-7 w-7 items-center justify-center rounded-md transition-colors ${attachmentTone(hasAttachment)}`}
        >
          <FileStack className="h-3.5 w-3.5" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="top">{tooltipText}</TooltipContent>
    </Tooltip>
  );
}

function ContextAttachmentButton({
  agent,
  onAgentUpdated,
}: {
  agent: Agent;
  onAgentUpdated?: (agent: Agent) => void;
}) {
  const { t } = useTranslation();
  const extensions = ATTACHMENT_EXTENSIONS.context!;
  const label = t("chatInput.contextDocs");
  const paths = agent.attachments.contextPaths ?? [];
  const hasAttachment = paths.length > 0;

  async function handleAdd() {
    try {
      const picked = await open({
        directory: false,
        multiple: true,
        title: t("chatInput.addDocument"),
        defaultPath: agent.folder ?? undefined,
        filters: [{ name: label, extensions }],
      });
      if (!picked) return;
      const list = Array.isArray(picked) ? picked : [picked];
      let latest: Agent = agent;
      for (const p of list) {
        latest = await agentApi.setAttachment(agent.id, "context", p);
      }
      onAgentUpdated?.(latest);
    } catch (e) {
      console.warn("context attach pick failed", e);
    }
  }

  async function handleRemove(path: string) {
    try {
      const updated = await agentApi.removeContextPath(agent.id, path);
      onAgentUpdated?.(updated);
    } catch (e) {
      console.warn("context remove failed", e);
    }
  }

  const tooltipText = hasAttachment
    ? paths.length === 1
      ? `${label}: ${paths[0]!.split(/[/\\]/).pop() ?? paths[0]!}`
      : `${label} (${paths.length})`
    : label;

  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label={tooltipText}
              className={`flex h-7 w-7 items-center justify-center rounded-md transition-colors ${attachmentTone(hasAttachment)}`}
            >
              <BookOpen className="h-3.5 w-3.5" />
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="top">{tooltipText}</TooltipContent>
      </Tooltip>
      <DropdownMenuContent side="top" align="start" className="w-72 p-2">
        <div className="mb-1 px-1 text-xs font-medium">{label}</div>
        {paths.length === 0 ? (
          <div className="px-1 py-1 text-xs text-muted-foreground">
            {t("chatInput.contextDocsEmpty")}
          </div>
        ) : (
          <div className="mb-2 flex flex-col gap-0.5">
            {paths.map((p) => {
              const fileName = p.split(/[/\\]/).pop() ?? p;
              return (
                <div
                  key={p}
                  className="flex items-center gap-1.5 rounded-sm px-1.5 py-1 hover:bg-accent/40"
                  title={p}
                >
                  <span className="min-w-0 flex-1 truncate text-xs">
                    {fileName}
                  </span>
                  <button
                    type="button"
                    onClick={() => handleRemove(p)}
                    aria-label={t("chatInput.removeDocument")}
                    className="shrink-0 rounded-sm p-0.5 text-muted-foreground hover:bg-destructive/20 hover:text-destructive"
                  >
                    <X className="h-3 w-3" />
                  </button>
                </div>
              );
            })}
          </div>
        )}
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="w-full gap-1.5"
          onClick={handleAdd}
        >
          <Plus className="h-3 w-3" />
          {t("chatInput.addDocument")}
        </Button>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
