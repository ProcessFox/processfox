import { Settings } from "lucide-react";
import { useTranslation } from "react-i18next";

import { AgentSwitcher } from "@/components/agent/AgentSwitcher";
import { ChatPane } from "@/components/chat/ChatPane";
import type { ChatError } from "@/lib/chatErrors";
import type { StarterPrompt } from "@/lib/starterPrompts";
import { FileTree } from "@/components/filetree/FileTree";
import { PreviewPane } from "@/components/preview/PreviewPane";
import { Button } from "@/components/ui/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import type { PendingToolCall } from "@/hooks/useAgentChat";
import type { Agent } from "@/types/agent";
import type { ChatMessage, PendingHitl, PendingQuestion } from "@/types/chat";

type Props = {
  agents: Agent[];
  activeAgent: Agent | null;
  selectedFile: { path: string; name: string } | null;
  messages: ChatMessage[];
  streamingText: string | null;
  streamingReasoning: string | null;
  pendingTools: PendingToolCall[];
  pendingHitl: PendingHitl | null;
  pendingQuestion: PendingQuestion | null;
  sending: boolean;
  chatError: ChatError | null;
  chatDisabled: boolean;
  chatDisabledReason: string | undefined;
  chatDisabledAction?: { label: string; run: () => void };
  starterPrompts: StarterPrompt[];
  inputPrefill?: { text: string; token: number };
  acceptsAttachments: string[];
  onAgentUpdated: (agent: Agent) => void;
  chatFooter?: { templateName: string | null; model: string | null };
  fileTreeRefresh: number;
  onSelectAgent: (agent: Agent) => void;
  onCreateAgent: () => void;
  onEditAgent: () => void;
  onClearHistory: () => void;
  onOpenSettings: () => void;
  onSelectFile: (path: string, name: string) => void;
  onClosePreview: () => void;
  onSendMessage: (text: string) => void;
  onCancelRun: () => void;
  onApproveHitl: () => void;
  onRejectHitl: () => void;
  onRespondToQuestion: (answer: string) => void;
  onPrefillInput: (text: string) => void;
  onDismissChatError: () => void;
};

export function Main({
  agents,
  activeAgent,
  selectedFile,
  messages,
  streamingText,
  streamingReasoning,
  pendingTools,
  pendingHitl,
  pendingQuestion,
  sending,
  chatError,
  chatDisabled,
  chatDisabledReason,
  chatDisabledAction,
  starterPrompts,
  inputPrefill,
  acceptsAttachments,
  onAgentUpdated,
  chatFooter,
  fileTreeRefresh,
  onSelectAgent,
  onCreateAgent,
  onEditAgent,
  onClearHistory,
  onOpenSettings,
  onSelectFile,
  onClosePreview,
  onSendMessage,
  onCancelRun,
  onApproveHitl,
  onRejectHitl,
  onRespondToQuestion,
  onPrefillInput,
  onDismissChatError,
}: Props) {
  const { t } = useTranslation();
  const showPreview = selectedFile !== null;

  return (
    <ResizablePanelGroup
      direction="horizontal"
      className="h-full w-full bg-background"
    >
      <ResizablePanel defaultSize={22} minSize={16} maxSize={36}>
        <div className="flex h-full flex-col border-r border-border bg-surface">
          <AgentSwitcher
            agents={agents}
            activeAgent={activeAgent}
            canClearHistory={!sending}
            onSelect={onSelectAgent}
            onCreate={onCreateAgent}
            onEdit={onEditAgent}
            onClearHistory={onClearHistory}
          />
          <div className="flex-1 overflow-hidden border-t border-border">
            <FileTree
              agentId={activeAgent?.id ?? null}
              agentFolder={activeAgent?.folder ?? null}
              refreshSignal={fileTreeRefresh}
              onSelectFile={onSelectFile}
              onRequestPickFolder={onEditAgent}
            />
          </div>
          {/* App-level settings live at the bottom of the sidebar (the
              desktop-app convention) — the header row is agent-scoped. */}
          <div className="border-t border-border p-2">
            <Button
              variant="ghost"
              size="sm"
              className="w-full justify-start gap-2 text-muted-foreground hover:text-foreground"
              onClick={onOpenSettings}
            >
              <Settings className="h-3.5 w-3.5" />
              {t("settings.title")}
            </Button>
          </div>
        </div>
      </ResizablePanel>

      <ResizableHandle />

      {showPreview && (
        <>
          <ResizablePanel defaultSize={38} minSize={20}>
            <PreviewPane
              agentId={activeAgent?.id ?? null}
              fileName={selectedFile?.name ?? null}
              filePath={selectedFile?.path ?? null}
              onClose={onClosePreview}
            />
          </ResizablePanel>
          <ResizableHandle />
        </>
      )}

      <ResizablePanel defaultSize={showPreview ? 40 : 78} minSize={30}>
        <ChatPane
          messages={messages}
          streamingText={streamingText}
          streamingReasoning={streamingReasoning}
          pendingTools={pendingTools}
          pendingHitl={pendingHitl}
          pendingQuestion={pendingQuestion}
          sending={sending}
          error={chatError}
          disabled={chatDisabled}
          disabledReason={chatDisabledReason}
          disabledAction={chatDisabledAction}
          starterPrompts={starterPrompts}
          inputPrefill={inputPrefill}
          agent={activeAgent}
          acceptsAttachments={acceptsAttachments}
          onAgentUpdated={onAgentUpdated}
          footer={chatFooter}
          onSend={onSendMessage}
          onCancel={onCancelRun}
          onApproveHitl={onApproveHitl}
          onRejectHitl={onRejectHitl}
          onRespondToQuestion={onRespondToQuestion}
          onPrefillInput={onPrefillInput}
          onDismissError={onDismissChatError}
          onOpenSettings={onOpenSettings}
        />
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}
