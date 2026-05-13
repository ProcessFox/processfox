import i18n from "i18next";

export type StarterPrompt = {
  skill: string;
  text: string;
};

type StarterDef = {
  skill: string;
  key: string;
};

const STARTER_DEFS: StarterDef[] = [
  { skill: "folder-search", key: "starter.folderSearch1" },
  { skill: "folder-search", key: "starter.folderSearch2" },
  { skill: "document-read", key: "starter.documentRead" },
  { skill: "document-extend", key: "starter.documentExtend" },
  { skill: "document-create-docx", key: "starter.documentCreateDocx" },
  { skill: "document-edit", key: "starter.documentEdit" },
  { skill: "document-from-template", key: "starter.documentFromTemplate" },
  { skill: "table-read", key: "starter.tableRead" },
  { skill: "table-update", key: "starter.tableUpdate" },
  { skill: "table-create", key: "starter.tableCreate" },
  { skill: "chat-context", key: "starter.chatContext" },
];

function resolvePrompts(): StarterPrompt[] {
  return STARTER_DEFS.map((d) => ({
    skill: d.skill,
    text: i18n.t(d.key),
  }));
}

export function pickStarterPrompts(
  activeSkillNames: string[],
  max = 4,
): StarterPrompt[] {
  const prompts = resolvePrompts();
  const active = new Set(activeSkillNames);
  const matching = prompts.filter((p) => active.has(p.skill));
  if (matching.length === 0) {
    return prompts.slice(0, Math.min(max, 2));
  }
  return matching.slice(0, max);
}
