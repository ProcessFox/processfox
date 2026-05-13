import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  Agent,
  AgentDraft,
  AgentUpdate,
  AttachmentKind,
} from "@/types/agent";
import type { ChatMessage, RunEvent, RunStarted } from "@/types/chat";
import type { FileEntry } from "@/types/file";
import type {
  CatalogEntry,
  DownloadEvent,
  HardwareInfo,
  InstalledModel,
} from "@/types/models";
import type { Settings } from "@/types/settings";
import type { Skill } from "@/types/skill";

export const agentApi = {
  list: () => invoke<Agent[]>("list_agents"),
  get: (id: string) => invoke<Agent>("get_agent", { id }),
  create: (draft: AgentDraft) => invoke<Agent>("create_agent", { draft }),
  update: (id: string, update: AgentUpdate) =>
    invoke<Agent>("update_agent", { id, update }),
  delete: (id: string) => invoke<void>("delete_agent", { id }),
  setAttachment: (agentId: string, kind: AttachmentKind, path: string | null) =>
    invoke<Agent>("set_agent_attachment", { agentId, kind, path }),
  removeContextPath: (agentId: string, path: string) =>
    invoke<Agent>("remove_agent_context_path", { agentId, path }),
  /** Watcher fires this when an agent's attachment was auto-cleared because
   *  the underlying file disappeared. Payload is the affected agent id. */
  subscribeAttachmentsChanged: (
    handler: (agentId: string) => void,
  ): Promise<UnlistenFn> =>
    listen<string>("agent-attachments-changed", (evt) => handler(evt.payload)),
};

export interface TextFileContent {
  content: string;
  /** Modification time in ms since UNIX epoch — pass back unchanged on save
   *  so the backend can detect external modifications. */
  mtime: number;
}

export interface TextWriteResult {
  mtime: number;
}

export interface DocxPreview {
  /** Sanitised body-only HTML — render in an iframe with `sandbox=""`. */
  html: string;
}

export interface XlsxPreview {
  sheets: string[];
  activeSheet: string;
  rows: string[][];
  totalRows: number;
  totalCols: number;
  truncated: boolean;
}

export interface SlidePreview {
  index: number;
  title: string | null;
  body: string[];
  notes: string[];
}

export interface PptxPreview {
  slides: SlidePreview[];
}

export const previewApi = {
  docx: (agentId: string, path: string) =>
    invoke<DocxPreview>("preview_docx", { agentId, path }),
  xlsx: (agentId: string, path: string, sheet?: string) =>
    invoke<XlsxPreview>("preview_xlsx", { agentId, path, sheet }),
  pptx: (agentId: string, path: string) =>
    invoke<PptxPreview>("preview_pptx", { agentId, path }),
};

export const fileApi = {
  listAgentFolder: (agentId: string, subPath?: string) =>
    invoke<FileEntry[]>("list_agent_folder", { agentId, subPath }),
  watchAgentFolder: (agentId: string) =>
    invoke<void>("watch_agent_folder", { agentId }),
  unwatchAgentFolder: () => invoke<void>("unwatch_agent_folder"),
  openLogsFolder: () => invoke<void>("open_logs_folder"),
  importFilesToAgent: (agentId: string, paths: string[]) =>
    invoke<string[]>("import_files_to_agent", { agentId, paths }),
  readTextFile: (agentId: string, path: string) =>
    invoke<TextFileContent>("read_text_file", { agentId, path }),
  writeTextFile: (
    agentId: string,
    path: string,
    content: string,
    expectedMtime: number,
  ) =>
    invoke<TextWriteResult>("write_text_file", {
      agentId,
      path,
      content,
      expectedMtime,
    }),
  /** Subscribe to FS-changed pings emitted by the watcher. The payload
   *  is empty `()`; callers should use it as a signal to refresh. */
  subscribeFsChanged: (handler: () => void): Promise<UnlistenFn> =>
    listen("fs-changed", () => handler()),
  /** Subscribe to drag-and-drop events from the OS. Payload is the list of
   *  absolute file paths the user dropped onto the window. */
  subscribeFilesDropped: (
    handler: (paths: string[]) => void,
  ): Promise<UnlistenFn> =>
    listen<string[]>("files-dropped", (evt) => handler(evt.payload)),
};

export const settingsApi = {
  get: () => invoke<Settings>("get_settings"),
  setDefaultProvider: (provider: string | null) =>
    invoke<Settings>("set_default_provider", { provider }),
  setDefaultModel: (provider: string, model: string | null) =>
    invoke<Settings>("set_default_model", { provider, model }),
  setFirstRunDone: () => invoke<Settings>("set_first_run_done"),
  setCustomOpenAiBaseUrl: (url: string | null) =>
    invoke<Settings>("set_custom_openai_base_url", { url }),
  setLocale: (locale: string | null) =>
    invoke<Settings>("set_locale", { locale }),
  availableProviders: () => invoke<string[]>("available_providers"),
};

export interface ValidationResult {
  ok: boolean;
  error?: string;
}

export const secretsApi = {
  setApiKey: (provider: string, value: string) =>
    invoke<void>("set_api_key", { provider, value }),
  hasApiKey: (provider: string) =>
    invoke<boolean>("has_api_key", { provider }),
  clearApiKey: (provider: string) =>
    invoke<void>("clear_api_key", { provider }),
  validateApiKey: (provider: string) =>
    invoke<ValidationResult>("validate_api_key", { provider }),
};

export const skillsApi = {
  list: () => invoke<Skill[]>("list_skills"),
};

export const modelsApi = {
  listCatalog: () => invoke<CatalogEntry[]>("list_catalog"),
  listInstalled: () => invoke<InstalledModel[]>("list_installed_models"),
  getHardwareInfo: () => invoke<HardwareInfo>("get_hardware_info"),
  downloadFromCatalog: (catalogId: string) =>
    invoke<void>("download_from_catalog", { catalogId }),
  downloadFromUrl: (downloadId: string, url: string, filename: string) =>
    invoke<void>("download_from_url", { downloadId, url, filename }),
  cancelDownload: (downloadId: string) =>
    invoke<void>("cancel_download", { downloadId }),
  deleteModel: (filename: string) =>
    invoke<void>("delete_model", { filename }),
  subscribeDownload: (
    downloadId: string,
    handler: (event: DownloadEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<DownloadEvent>(
      `model:download:${sanitizeEventSegment(downloadId)}`,
      (evt) => handler(evt.payload),
    ),
};

/**
 * Tauri event names only allow alphanumeric, `-`, `/`, `:`, `_`. Mirror the
 * backend's sanitizer so catalog IDs or custom download IDs with other
 * characters can still be used as logical identifiers.
 */
function sanitizeEventSegment(segment: string): string {
  return segment.replace(/[^a-zA-Z0-9\-\/:_]/g, "_");
}

export const chatApi = {
  listMessages: (agentId: string) =>
    invoke<ChatMessage[]>("list_messages", { agentId }),

  sendMessage: (params: {
    agentId: string;
    provider: string;
    modelId: string;
    text: string;
  }) => invoke<RunStarted>("send_message", params),

  cancelRun: (runId: string) => invoke<void>("cancel_run", { runId }),

  approveHitl: (hitlId: string) =>
    invoke<void>("approve_hitl", { hitlId }),
  rejectHitl: (hitlId: string, reason?: string) =>
    invoke<void>("reject_hitl", { hitlId, reason }),
  respondToQuestion: (questionId: string, answer: string) =>
    invoke<void>("respond_to_question", { questionId, answer }),

  subscribeRun: (
    runId: string,
    handler: (event: RunEvent) => void,
  ): Promise<UnlistenFn> =>
    listen<RunEvent>(`chat:run:${runId}`, (evt) => handler(evt.payload)),
};
