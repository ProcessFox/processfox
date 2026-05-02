//! Stateless LLM inference for an agent's "background worker" — used by
//! fan-out tools that need a focused, per-item generation step without
//! involving the parent ReAct loop. Resolves the worker's provider /
//! model / system-prompt from the agent's `delegation_profile`, falling
//! back to the agent's own settings and finally to the global defaults.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::core::agent::{Agent, ModelRef};
use crate::core::error::{CoreError, CoreResult};
use crate::core::llm::{
    ChatRole, ChatTurn, FinishReason, GenerateRequest, LlmEvent, ProviderRegistry, TokenUsage,
};
use crate::core::settings::SettingsStore;

const DEFAULT_MAX_TOKENS: u32 = 1024;

/// Baseline system prompt for an agent's background worker when the user has
/// not set their own override. Drives terse, single-result output suitable
/// for filling a column or producing one item per row — *not* a conversational
/// reply. Cloud LLMs (especially Claude) default to multi-option, headlined,
/// Markdown-formatted answers; this prompt steers them away from that for the
/// bulk-fan-out use case. Users who want different behaviour set the override
/// in the agent's worker settings.
pub const DEFAULT_WORKER_SYSTEM_PROMPT: &str = "Du bist ein Hintergrund-Worker für Bulk-Aufgaben. \
Antworte mit *genau einem* Ergebnis. Keine Optionen, keine Alternativenliste, keine Aufzählungen, \
keine Markdown-Formatierung, kein Vorwort, keine Erklärung, kein „Hier ist …\". Liefere nur das \
angefragte Ergebnis als reinen Text. Antworte in derselben Sprache wie die Aufgabe.";

#[derive(Debug, Clone)]
pub struct DelegationRunner {
    registry: ProviderRegistry,
    settings: SettingsStore,
}

#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    pub provider_id: String,
    pub model_id: String,
    pub system_prompt: String,
}

impl DelegationRunner {
    pub fn new(registry: ProviderRegistry, settings: SettingsStore) -> Self {
        Self { registry, settings }
    }

    /// Resolve the concrete provider, model id and system prompt to use for
    /// this agent's worker. Returns `Err` if the profile is missing or
    /// disabled, or if no model can be resolved at all.
    pub fn resolve_for(&self, agent: &Agent) -> CoreResult<ResolvedProfile> {
        let profile = agent
            .delegation_profile
            .as_ref()
            .filter(|p| p.enabled)
            .ok_or_else(|| {
                CoreError::Llm("Hintergrund-Worker für diesen Agenten ist nicht aktiviert.".into())
            })?;

        // Worker default is the baked-in terse prompt, *not* the parent
        // agent's prompt. The parent's system prompt is tuned for
        // conversation (helpful, thorough, Markdown-friendly), which is
        // exactly the wrong shape for bulk fan-out where we want one
        // unadorned result per item.
        let system_prompt = profile
            .system_prompt_override
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_WORKER_SYSTEM_PROMPT.to_string());

        let model = profile
            .model_override
            .clone()
            .or_else(|| agent.model.clone());

        let (provider_id, model_id) = match model {
            Some(ModelRef::Local { id }) => ("local".to_string(), id),
            Some(ModelRef::Cloud { provider, id }) => (provider, id),
            None => {
                let s = self.settings.load()?;
                let provider = s.default_provider.clone().ok_or_else(|| {
                    CoreError::Llm("Kein Default-Provider in den Einstellungen gesetzt.".into())
                })?;
                let model_id = s.default_models.get(&provider).cloned().ok_or_else(|| {
                    CoreError::Llm(format!(
                        "Kein Default-Modell für Provider {provider} hinterlegt."
                    ))
                })?;
                (provider, model_id)
            }
        };

        Ok(ResolvedProfile {
            provider_id,
            model_id,
            system_prompt,
        })
    }

    /// Run one stateless single-turn inference. No tools, no history,
    /// reasoning channels are discarded. Returns the full text of the
    /// model's response together with whatever `TokenUsage` the provider
    /// reported (some compat backends report nothing — caller must handle
    /// `None` honestly rather than logging zeros). Cancellation is checked
    /// promptly.
    pub async fn run_one(
        &self,
        resolved: &ResolvedProfile,
        prompt: String,
        cancel: CancellationToken,
    ) -> CoreResult<(String, Option<TokenUsage>)> {
        let provider = self.registry.get(&resolved.provider_id)?;
        let (tx, mut rx) = mpsc::channel::<LlmEvent>(64);

        let system_prompt = if resolved.system_prompt.trim().is_empty() {
            None
        } else {
            Some(resolved.system_prompt.clone())
        };

        let request = GenerateRequest {
            model_id: resolved.model_id.clone(),
            system_prompt,
            turns: vec![ChatTurn {
                role: ChatRole::User,
                content: prompt,
                tool_calls: Vec::new(),
                tool_results: Vec::new(),
            }],
            tools: Vec::new(),
            max_tokens: DEFAULT_MAX_TOKENS,
            temperature: None,
        };

        let provider_bg: Arc<dyn crate::core::llm::LlmProvider> = provider.clone();
        let cancel_for_provider = cancel.clone();
        let gen_task =
            tokio::spawn(
                async move { provider_bg.generate(request, tx, cancel_for_provider).await },
            );

        let mut out = String::new();
        let mut terminal_error: Option<(String, String)> = None;
        let mut finish_reason: Option<FinishReason> = None;
        let mut usage: Option<TokenUsage> = None;

        while let Some(event) = rx.recv().await {
            match event {
                LlmEvent::TextDelta { text } => out.push_str(&text),
                // Workers run without tools, so a tool call would be a model
                // mistake; reasoning has no UI to surface to. Both ignored.
                LlmEvent::ReasoningDelta { .. } => {}
                LlmEvent::ToolCall(_) => {}
                // Per-item logging is intentionally suppressed — the caller
                // aggregates these into a single fan-out summary log.
                LlmEvent::Usage(u) => usage = Some(u),
                LlmEvent::Finish { reason } => finish_reason = Some(reason),
                LlmEvent::Error { code, message } => {
                    terminal_error = Some((code, message));
                }
            }
        }

        let _ = gen_task.await;

        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        if let Some((code, message)) = terminal_error {
            return Err(CoreError::Llm(format!("{code}: {message}")));
        }
        // FinishReason::MaxTokens is accepted — caller gets the partial text.
        let _ = finish_reason;
        Ok((out, usage))
    }
}
