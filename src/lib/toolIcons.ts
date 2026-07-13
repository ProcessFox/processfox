import {
  FileEdit,
  FilePen,
  FilePlus,
  FileSearch,
  FileSignature,
  FileSpreadsheet,
  FileStack,
  FileText,
  FileType,
  FolderOpen,
  GraduationCap,
  MessageCircleQuestion,
  Workflow,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import i18n from "i18next";

const TOOL_ICONS: Record<string, LucideIcon> = {
  list_folder: FolderOpen,
  read_file: FileText,
  grep_in_files: FileSearch,
  read_docx: FileText,
  read_xlsx_range: FileSpreadsheet,
  read_pdf: FileType,
  write_docx: FilePlus,
  write_xlsx: FilePlus,
  update_xlsx_cell: FilePen,
  append_to_md: FileSignature,
  append_to_docx: FileSignature,
  rewrite_file: FileEdit,
  ask_user: MessageCircleQuestion,
  read_skill: GraduationCap,
  write_docx_from_template: FileStack,
  delegate_into_xlsx_column: Workflow,
};

export function iconForTool(name: string): LucideIcon {
  return TOOL_ICONS[name] ?? Wrench;
}

/** Human-readable, localized label for a raw tool name. Raw names like
 *  `grep_in_files` are jargon for the beginner audience — components show
 *  this label and keep the raw name as a tooltip. Unknown tools fall back
 *  to the raw name so nothing ever renders blank. */
export function labelForTool(name: string): string {
  const key = `tool.${name}`;
  return i18n.exists(key) ? i18n.t(key) : name;
}
