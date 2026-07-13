/** Classification of raw chat/run errors into beginner-friendly buckets.
 *
 * The backend forwards provider errors mostly verbatim (`llm_error` with a
 * message like "http_error: Anthropic 401: {json}"), so classification has
 * to pattern-match the message text, not just the code. Order matters:
 * more specific patterns (auth, rate limit) run before the catch-alls
 * (overloaded, network).
 */
export type ChatErrorKind =
  | "auth"
  | "rateLimit"
  | "modelNotFound"
  | "context"
  | "overloaded"
  | "network"
  | "generic";

export type ChatError = {
  code: string | null;
  message: string;
};

const PATTERNS: { kind: ChatErrorKind; re: RegExp }[] = [
  {
    kind: "auth",
    re: /\b401\b|\b403\b|invalid[ _-]?(api[ _-]?)?key|unauthorized|authentication|permission[ _-]?error/i,
  },
  { kind: "rateLimit", re: /\b429\b|rate[ _-]?limit|quota/i },
  {
    kind: "modelNotFound",
    re: /\b404\b|not_found_error|model.{0,40}(not found|does not exist)|unknown model/i,
  },
  {
    kind: "context",
    re: /context[ _-]?(length|window)|prompt is too long|too many tokens|maximum.{0,20}tokens/i,
  },
  { kind: "overloaded", re: /overloaded|\b529\b|\b503\b|\b502\b|\b500\b|internal server/i },
  {
    kind: "network",
    re: /error sending request|network|connection|connect|dns|timed?[ _-]?out|unreachable/i,
  },
];

export function classifyChatError(error: ChatError): ChatErrorKind {
  if (error.code === "missing_api_key") return "auth";
  for (const { kind, re } of PATTERNS) {
    if (re.test(error.message)) return kind;
  }
  return "generic";
}

/** i18n key with the friendly one-liner for a classified error. */
export function chatErrorI18nKey(kind: ChatErrorKind): string {
  return `errors.${kind}`;
}

/** Errors the user can fix in the settings dialog get an inline
 *  "open settings" action next to the log/dismiss buttons. */
export function chatErrorWantsSettings(kind: ChatErrorKind): boolean {
  return kind === "auth" || kind === "modelNotFound";
}
