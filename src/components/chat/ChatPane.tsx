import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertCircle,
  Check,
  ChevronRight,
  Copy,
  Loader2,
  Square,
  X,
} from "lucide-react";

import { AskUserCard } from "@/components/chat/AskUserCard";
import { ChatInput } from "@/components/chat/ChatInput";
import { fileApi } from "@/lib/tauri";
import type { StarterPrompt } from "@/lib/starterPrompts";
import {
  chatErrorI18nKey,
  chatErrorWantsSettings,
  classifyChatError,
  type ChatError,
} from "@/lib/chatErrors";
import { HitlCard } from "@/components/chat/HitlCard";
import { MessageMarkdown } from "@/components/chat/MessageMarkdown";
import { ReasoningChip } from "@/components/chat/ReasoningChip";
import { ToolCallChip } from "@/components/chat/ToolCallChip";
import { Button } from "@/components/ui/button";
import type { PendingToolCall } from "@/hooks/useAgentChat";
import type { Agent } from "@/types/agent";
import type { ChatMessage, PendingHitl, PendingQuestion } from "@/types/chat";

type Props = {
  messages: ChatMessage[];
  streamingText: string | null;
  streamingReasoning: string | null;
  pendingTools: PendingToolCall[];
  pendingHitl: PendingHitl | null;
  pendingQuestion: PendingQuestion | null;
  sending: boolean;
  error: ChatError | null;
  disabled?: boolean;
  disabledReason?: string;
  /** Action that actually fixes the disabled reason (create an agent, open
   *  the right settings tab, …) — shown as the banner's button. */
  disabledAction?: { label: string; run: () => void };
  starterPrompts?: StarterPrompt[];
  inputPrefill?: { text: string; token: number };
  agent?: Agent | null;
  acceptsAttachments?: string[];
  onAgentUpdated?: (agent: Agent) => void;
  footer?: { templateName: string | null; model: string | null };
  onSend: (text: string) => void;
  onCancel: () => void;
  onApproveHitl: () => void;
  onRejectHitl: () => void;
  onRespondToQuestion: (answer: string) => void;
  onPrefillInput?: (text: string) => void;
  onDismissError: () => void;
  onOpenSettings?: () => void;
};

export function ChatPane({
  messages,
  streamingText,
  streamingReasoning,
  pendingTools,
  pendingHitl,
  pendingQuestion,
  sending,
  error,
  disabled,
  disabledReason,
  disabledAction,
  starterPrompts,
  inputPrefill,
  agent,
  acceptsAttachments,
  onAgentUpdated,
  footer,
  onSend,
  onCancel,
  onApproveHitl,
  onRejectHitl,
  onRespondToQuestion,
  onPrefillInput,
  onDismissError,
  onOpenSettings,
}: Props) {
  const { t, i18n } = useTranslation();
  const scrollRef = useRef<HTMLDivElement>(null);
  // Follow the stream only while the user is parked at the bottom. Once
  // they scroll up to re-read something, new deltas must not yank them
  // back down; returning near the bottom re-engages following.
  const stickToBottomRef = useRef(true);

  useEffect(() => {
    const el = scrollRef.current;
    if (el && stickToBottomRef.current) el.scrollTop = el.scrollHeight;
  }, [messages.length, streamingText, streamingReasoning, pendingTools.length]);

  const handleScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    stickToBottomRef.current =
      el.scrollHeight - el.scrollTop - el.clientHeight < 40;
  };

  const handleSend = (text: string) => {
    // Sending implies "show me the reply" — re-engage following even if
    // the user had scrolled up before.
    stickToBottomRef.current = true;
    onSend(text);
  };

  const showEmpty =
    messages.length === 0 &&
    streamingText === null &&
    !sending &&
    pendingTools.length === 0;

  // Filter out "tool" messages from display — they're implementation detail;
  // the relevant info lives on the preceding assistant message's tool_calls.
  const visibleMessages = messages.filter((m) => m.role !== "tool");

  return (
    <div className="flex h-full flex-col bg-background">
      <div
        ref={scrollRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto px-4 py-4"
      >
        {showEmpty ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
            <div className="text-sm font-medium">{t("chat.emptyTitle")}</div>
            {starterPrompts && starterPrompts.length > 0 ? (
              <>
                <div className="text-xs text-muted-foreground">
                  {t("chat.trySuggestion")}
                </div>
                <div className="flex max-w-md flex-col gap-1.5">
                  {starterPrompts.map((p, i) => (
                    <button
                      key={`${p.skill}-${i}`}
                      onClick={() => onPrefillInput?.(p.text)}
                      disabled={disabled}
                      className="rounded-md border border-border bg-background px-3 py-2 text-left text-xs text-foreground hover:border-ring hover:bg-accent/40 disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      „{p.text}"
                    </button>
                  ))}
                </div>
              </>
            ) : (
              <div className="text-xs text-muted-foreground">
                {t("chat.noSkillsHint")}
              </div>
            )}
          </div>
        ) : (
          <div className="flex flex-col gap-3">
            {visibleMessages.map((m, i) => {
              const divider = dividerLabelBetween(
                visibleMessages[i - 1],
                m,
                t,
                i18n.language,
              );
              return (
                <div key={m.id} className="flex flex-col gap-3">
                  {divider && <DateDivider label={divider} />}
                  <MessageBlock
                    message={m}
                    toolResults={findToolResults(m, messages)}
                  />
                </div>
              );
            })}

            {streamingReasoning !== null &&
              streamingReasoning.length > 0 && (
                <ReasoningChip text={streamingReasoning} streaming />
              )}

            {pendingTools.length > 0 && (
              <div className="flex flex-col gap-1.5">
                {pendingTools.map((t) => (
                  <ToolCallChip
                    key={t.id}
                    name={t.name}
                    status={t.status}
                    arguments={t.arguments}
                    result={t.content}
                    delegation={t.delegation}
                  />
                ))}
              </div>
            )}

            {streamingText !== null && streamingText.length > 0 && (
              <StreamingBubble text={streamingText} />
            )}

            {pendingHitl && (
              <HitlCard
                hitl={pendingHitl}
                onApprove={onApproveHitl}
                onReject={onRejectHitl}
              />
            )}

            {pendingQuestion && (
              <AskUserCard
                question={pendingQuestion}
                onRespond={onRespondToQuestion}
              />
            )}
          </div>
        )}
      </div>

      {error && (
        <ErrorBanner
          error={error}
          onOpenSettings={onOpenSettings}
          onDismiss={onDismissError}
        />
      )}

      {sending && (
        <div className="flex items-center justify-between gap-2 border-t border-border bg-muted/40 px-4 py-2 text-xs text-muted-foreground">
          <div className="flex items-center gap-2">
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
            {pendingHitl
              ? t("chat.statusApproval")
              : pendingQuestion
                ? t("chat.statusQuestion")
                : pendingTools.some((pt) => pt.status === "running")
                  ? t("chat.statusTool")
                  : t("chat.statusGenerating")}
          </div>
          <Button size="sm" variant="outline" onClick={onCancel} className="gap-1.5">
            <Square className="h-3 w-3" />
            {t("common.stop")}
          </Button>
        </div>
      )}

      {disabled && disabledReason && !sending && (
        <div className="flex items-start gap-2 border-t border-amber-500/40 bg-amber-500/15 px-4 py-2 text-xs text-amber-800 dark:text-amber-200">
          <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <div className="flex-1">{disabledReason}</div>
          {disabledAction && (
            <button
              onClick={disabledAction.run}
              className="shrink-0 rounded-sm border border-amber-500/40 bg-amber-500/15 px-2 py-0.5 text-xs hover:bg-amber-500/20"
            >
              {disabledAction.label}
            </button>
          )}
        </div>
      )}

      <ChatInput
        disabled={disabled || sending}
        onSend={handleSend}
        prefill={inputPrefill}
        agent={agent ?? null}
        acceptsAttachments={acceptsAttachments}
        onAgentUpdated={onAgentUpdated}
        footer={footer}
      />
    </div>
  );
}

/** Friendly error strip above the input. Beginners get a plain-language
 *  summary and a matching action; the raw provider payload stays reachable
 *  behind a collapsed "details" toggle for bug reports. */
function ErrorBanner({
  error,
  onOpenSettings,
  onDismiss,
}: {
  error: ChatError;
  onOpenSettings?: () => void;
  onDismiss: () => void;
}) {
  const { t } = useTranslation();
  const kind = classifyChatError(error);
  return (
    <div className="flex items-start gap-2 border-t border-destructive/30 bg-destructive/15 px-4 py-2 text-xs text-destructive">
      <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
      <div className="min-w-0 flex-1">
        <div>{t(chatErrorI18nKey(kind))}</div>
        <details className="group mt-1">
          <summary className="flex cursor-pointer list-none items-center gap-1 opacity-70 hover:opacity-100 [&::-webkit-details-marker]:hidden">
            <ChevronRight className="h-3 w-3 shrink-0 transition-transform group-open:rotate-90" />
            {t("errors.details")}
          </summary>
          <pre className="mt-1 max-h-24 overflow-auto whitespace-pre-wrap break-words rounded-sm bg-background/40 p-1.5 font-mono text-[11px]">
            {error.code ? `${error.code}: ${error.message}` : error.message}
          </pre>
        </details>
      </div>
      {chatErrorWantsSettings(kind) && onOpenSettings && (
        <button
          onClick={onOpenSettings}
          className="shrink-0 rounded-sm border border-destructive/40 bg-destructive/15 px-2 py-0.5 text-xs hover:bg-destructive/20"
        >
          {t("chat.openSettings")}
        </button>
      )}
      <button
        onClick={() => fileApi.openLogsFolder().catch(() => {})}
        className="shrink-0 rounded-sm border border-destructive/40 bg-destructive/15 px-2 py-0.5 text-xs hover:bg-destructive/20"
        title={t("chat.openLogsInFinder")}
      >
        {t("chat.openLogs")}
      </button>
      <button
        onClick={onDismiss}
        className="text-destructive/70 hover:text-destructive"
        title={t("common.close")}
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

/** Label for a date divider between two adjacent messages, or null when
 *  none is needed. The very first message only gets a divider when it is
 *  older than today — an all-fresh chat shouldn't start with "Today". */
function dividerLabelBetween(
  prev: ChatMessage | undefined,
  curr: ChatMessage,
  t: (key: string) => string,
  locale: string,
): string | null {
  const currDay = new Date(curr.createdAt).toDateString();
  if (prev) {
    if (new Date(prev.createdAt).toDateString() === currDay) return null;
  } else if (currDay === new Date().toDateString()) {
    return null;
  }
  return dayLabel(curr.createdAt, t, locale);
}

function dayLabel(
  iso: string,
  t: (key: string) => string,
  locale: string,
): string {
  const d = new Date(iso);
  const startOfDay = (x: Date) =>
    new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diffDays = Math.round(
    (startOfDay(new Date()) - startOfDay(d)) / 86_400_000,
  );
  if (diffDays === 0) return t("chat.today");
  if (diffDays === 1) return t("chat.yesterday");
  return d.toLocaleDateString(locale, {
    day: "numeric",
    month: "long",
    year: "numeric",
  });
}

function DateDivider({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-3 py-1">
      <div className="h-px flex-1 bg-border" />
      <span className="shrink-0 text-[11px] text-muted-foreground">
        {label}
      </span>
      <div className="h-px flex-1 bg-border" />
    </div>
  );
}

/** Find persisted tool results for an assistant message's tool calls by
 *  scanning subsequent tool-role messages in the same history. */
function findToolResults(
  message: ChatMessage,
  all: ChatMessage[],
): Record<string, { content: string; isError: boolean }> {
  const results: Record<string, { content: string; isError: boolean }> = {};
  if (!message.toolCalls || message.toolCalls.length === 0) return results;
  const idx = all.findIndex((m) => m.id === message.id);
  if (idx < 0) return results;
  for (const later of all.slice(idx + 1)) {
    if (later.role !== "tool") break;
    for (const tr of later.toolResults ?? []) {
      results[tr.toolUseId] = { content: tr.content, isError: tr.isError };
    }
  }
  return results;
}

function MessageBlock({
  message,
  toolResults,
}: {
  message: ChatMessage;
  toolResults: Record<string, { content: string; isError: boolean }>;
}) {
  const { t } = useTranslation();
  const isUser = message.role === "user";
  const hasToolCalls = (message.toolCalls?.length ?? 0) > 0;
  const hasText = message.content.trim().length > 0;

  if (isUser) {
    return (
      <div className="flex justify-end">
        <div className="max-w-[85%] whitespace-pre-wrap rounded-md bg-primary/10 px-3 py-2 text-sm text-foreground">
          {message.content}
        </div>
      </div>
    );
  }

  // Assistant message: render reasoning chip + tool chips, then the text.
  const reasoning = message.reasoning?.trim();
  return (
    <div className="flex flex-col gap-1.5">
      {reasoning && reasoning.length > 0 && (
        <ReasoningChip text={reasoning} />
      )}
      {hasToolCalls && (
        <div className="flex flex-col gap-1">
          {message.toolCalls!.map((tc) => {
            const res = toolResults[tc.id];
            const status = res
              ? res.isError
                ? "error"
                : "done"
              : "error";
            return (
              <ToolCallChip
                key={tc.id}
                name={tc.name}
                status={status}
                arguments={tc.arguments}
                result={res?.content ?? t("chat.toolAborted")}
              />
            );
          })}
        </div>
      )}
      {hasText && <AssistantBubble text={message.content} />}
    </div>
  );
}

/** Assistant message bubble with a hover-revealed copy button. We track
 *  hover via React state instead of CSS `group-hover` because the Tauri
 *  WebView occasionally fails to clear the `:hover` pseudo-class when the
 *  cursor leaves quickly, leaving the button stuck visible. */
function AssistantBubble({ text }: { text: string }) {
  const { t } = useTranslation();
  const [hovered, setHovered] = useState(false);
  const [copied, setCopied] = useState(false);

  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      console.warn("clipboard write failed", e);
    }
  };

  // Keep the button shown for the duration of the "Kopiert!" feedback even
  // if the user moved the cursor away mid-click.
  const visible = hovered || copied;

  return (
    <div
      className="flex justify-start"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <div className="max-w-[85%] rounded-md bg-muted px-3 py-2 text-sm text-foreground">
        <MessageMarkdown text={text} />
      </div>
      <button
        onClick={onCopy}
        title={copied ? t("chat.copied") : t("chat.copyToClipboard")}
        className={`ml-1.5 mt-1.5 h-6 w-6 shrink-0 rounded-sm text-muted-foreground transition-opacity hover:bg-accent/40 hover:text-foreground ${
          visible ? "opacity-100" : "pointer-events-none opacity-0"
        }`}
      >
        {copied ? (
          <Check className="mx-auto h-3.5 w-3.5" />
        ) : (
          <Copy className="mx-auto h-3.5 w-3.5" />
        )}
      </button>
    </div>
  );
}

function StreamingBubble({ text }: { text: string }) {
  return (
    <div className="flex justify-start">
      <div className="max-w-[85%] rounded-md bg-muted px-3 py-2 text-sm text-foreground">
        <MessageMarkdown text={text} />
        <span className="ml-0.5 inline-block h-3 w-1 translate-y-0.5 animate-pulse bg-foreground/60" />
      </div>
    </div>
  );
}
