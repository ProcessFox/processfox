use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::repo::{ChatMessage, ChatRepo, MessageRole};
use crate::core::agent::Agent;
use crate::core::error::{CoreError, CoreResult};
use crate::core::llm::{
    ChatRole, ChatTurn, FinishReason, GenerateRequest, LlmEvent, ProviderRegistry, TokenUsage,
    ToolCall, ToolResult,
};
use crate::core::skill::SkillRegistry;
use crate::core::tool::{HitlDecision, HitlPreview, ToolContext, ToolRegistry, ToolSchema};
use tokio::sync::oneshot;

pub type RunId = String;

/// Max number of prior messages included when prompting the model.
const HISTORY_WINDOW: usize = 20;
const DEFAULT_MAX_TOKENS: u32 = 4096;
const MAX_REACT_ITERATIONS: u32 = 12;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStarted {
    pub run_id: RunId,
    pub assistant_message_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum RunEvent {
    Delta {
        text: String,
    },
    ReasoningDelta {
        text: String,
    },
    ToolCallStarted {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolCallCompleted {
        id: String,
        content: String,
        is_error: bool,
    },
    /// Run is paused waiting for the user to approve or reject a write
    /// operation. The runner resumes once `ChatRunner::resolve_hitl` is
    /// called via the `approve_hitl` / `reject_hitl` Tauri command.
    HitlRequest {
        hitl_id: String,
        tool_call_id: String,
        tool_name: String,
        preview: HitlPreview,
    },
    /// User has decided on a pending HITL request. Sent for transparency —
    /// the actual outcome shows up as a regular `ToolCallCompleted` once
    /// the tool finished (or never, if rejected).
    HitlResolved {
        hitl_id: String,
        decision: HitlDecision,
    },
    /// Run is paused waiting for the user to type an answer to a clarifying
    /// question raised by the `ask_user` tool. The runner resumes once
    /// `ChatRunner::resolve_question` is called via `respond_to_question`.
    AskUserRequest {
        question_id: String,
        tool_call_id: String,
        question: String,
    },
    /// User has typed an answer to a pending ask_user. The answer is also
    /// included in the corresponding `ToolCallCompleted` so frontends that
    /// only listen to the tool channel still see it.
    AskUserResolved {
        question_id: String,
        answer: String,
    },
    /// A fan-out tool (e.g. `delegate_into_xlsx_column`) has begun
    /// processing N items. Tied to the parent `tool_call_id` so the UI can
    /// render progress inside the existing tool-call card.
    DelegationStarted {
        tool_call_id: String,
        total: u32,
    },
    /// One item of a fan-out has finished. `index` is 1-based.
    DelegationItemDone {
        tool_call_id: String,
        index: u32,
        total: u32,
        item_label: String,
    },
    /// One item failed (e.g. provider error). The fan-out continues with the
    /// remaining items unless the run was cancelled.
    DelegationItemFailed {
        tool_call_id: String,
        index: u32,
        total: u32,
        item_label: String,
        error: String,
    },
    /// Fan-out finished — terminal counter event for this tool_call_id.
    DelegationFinished {
        tool_call_id: String,
        succeeded: u32,
        failed: u32,
    },
    Finish {
        reason: String,
        message: ChatMessage,
    },
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug)]
pub struct ChatRunHandle {
    pub cancel: CancellationToken,
}

/// Shared map of pending HITL requests. The chat-runner inserts a one-shot
/// sender when it pauses for approval; the `approve_hitl` / `reject_hitl`
/// commands look the request up by `hitl_id` and resolve it.
type PendingHitls = Arc<Mutex<HashMap<String, oneshot::Sender<HitlDecision>>>>;

/// Shared map of pending `ask_user` questions. Same shape as PendingHitls but
/// the payload is a free-form answer string rather than an Approve/Reject
/// decision.
type PendingQuestions = Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>;

#[derive(Clone, Debug)]
pub struct ChatRunner {
    app: AppHandle,
    repo: ChatRepo,
    registry: ProviderRegistry,
    tools: ToolRegistry,
    skills: SkillRegistry,
    active: Arc<Mutex<HashMap<RunId, ChatRunHandle>>>,
    pending_hitls: PendingHitls,
    pending_questions: PendingQuestions,
}

impl ChatRunner {
    pub fn new(
        app: AppHandle,
        repo: ChatRepo,
        registry: ProviderRegistry,
        tools: ToolRegistry,
        skills: SkillRegistry,
    ) -> Self {
        Self {
            app,
            repo,
            registry,
            tools,
            skills,
            active: Arc::new(Mutex::new(HashMap::new())),
            pending_hitls: Arc::new(Mutex::new(HashMap::new())),
            pending_questions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Resolve a pending HITL request with the user's decision. Looks the
    /// request up by `hitl_id`; if it's still pending, sends the decision
    /// to the chat-runner task that's waiting on it.
    pub async fn resolve_hitl(&self, hitl_id: &str, decision: HitlDecision) -> bool {
        let mut guard = self.pending_hitls.lock().await;
        if let Some(tx) = guard.remove(hitl_id) {
            let _ = tx.send(decision);
            true
        } else {
            false
        }
    }

    /// Resolve a pending ask_user question with the user's typed answer.
    pub async fn resolve_question(&self, question_id: &str, answer: String) -> bool {
        let mut guard = self.pending_questions.lock().await;
        if let Some(tx) = guard.remove(question_id) {
            let _ = tx.send(answer);
            true
        } else {
            false
        }
    }

    /// Kick off a ReAct-style run: persist the user message, then loop LLM →
    /// tool calls → tool results → LLM until the model finishes without
    /// another tool call (or we hit the iteration cap).
    pub async fn start(
        &self,
        agent: Agent,
        provider_id: String,
        model_id: String,
        user_text: String,
    ) -> CoreResult<RunStarted> {
        let user_msg = ChatMessage::new(MessageRole::User, user_text.clone());
        self.repo.append(&agent.id, &user_msg)?;

        let provider = self.registry.get(&provider_id)?;
        let run_id = Uuid::new_v4().to_string();
        let assistant_id = Uuid::new_v4().to_string();
        let cancel = CancellationToken::new();

        {
            let mut active = self.active.lock().await;
            active.insert(
                run_id.clone(),
                ChatRunHandle {
                    cancel: cancel.clone(),
                },
            );
        }

        let app = self.app.clone();
        let repo = self.repo.clone();
        let tools = self.tools.clone();
        let skills = self.skills.clone();
        let pending_hitls = self.pending_hitls.clone();
        let pending_questions = self.pending_questions.clone();
        let agent_id = agent.id.clone();
        let run_id_bg = run_id.clone();
        let assistant_id_bg = assistant_id.clone();
        let active = self.active.clone();

        tokio::spawn(async move {
            use futures_util::FutureExt;
            use std::panic::AssertUnwindSafe;

            let channel = format!("chat:run:{run_id_bg}");

            // Wrap the loop in catch_unwind so a panic deep in the provider
            // (e.g. an unsupported GGUF architecture in the inference engine)
            // surfaces as a clean error event in the UI instead of a silent
            // worker-thread death.
            let outcome = AssertUnwindSafe(react_loop(
                &app,
                &channel,
                provider,
                &repo,
                &tools,
                &skills,
                &pending_hitls,
                &pending_questions,
                &agent,
                &model_id,
                &assistant_id_bg,
                cancel.clone(),
            ))
            .catch_unwind()
            .await;

            match outcome {
                Ok(Ok(final_msg)) => {
                    let _ = app.emit(
                        &channel,
                        RunEvent::Finish {
                            reason: "stop".to_string(),
                            message: final_msg,
                        },
                    );
                }
                Ok(Err(e)) => {
                    let code = match &e {
                        CoreError::Cancelled => "cancelled",
                        _ => "llm_error",
                    };
                    let _ = app.emit(
                        &channel,
                        RunEvent::Error {
                            code: code.to_string(),
                            message: e.to_string(),
                        },
                    );
                }
                Err(panic) => {
                    let panic_msg = panic
                        .downcast_ref::<&str>()
                        .map(|s| (*s).to_string())
                        .or_else(|| panic.downcast_ref::<String>().cloned())
                        .unwrap_or_else(|| "panic without payload".to_string());
                    tracing::error!(panic = %panic_msg, "chat run panicked");
                    let _ = app.emit(
                        &channel,
                        RunEvent::Error {
                            code: "panic".to_string(),
                            message: format!(
                                "Provider crashte intern. Bitte einen anderen Provider/ein anderes Modell wählen. Detail: {panic_msg}"
                            ),
                        },
                    );
                }
            }

            let _ = agent_id;
            let mut active = active.lock().await;
            active.remove(&run_id_bg);
        });

        Ok(RunStarted {
            run_id,
            assistant_message_id: assistant_id,
        })
    }

    pub async fn cancel(&self, run_id: &str) {
        let active = self.active.lock().await;
        if let Some(handle) = active.get(run_id) {
            handle.cancel.cancel();
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn react_loop(
    app: &AppHandle,
    channel: &str,
    provider: Arc<dyn crate::core::llm::LlmProvider>,
    repo: &ChatRepo,
    tools: &ToolRegistry,
    skills: &SkillRegistry,
    pending_hitls: &PendingHitls,
    pending_questions: &PendingQuestions,
    agent: &Agent,
    model_id: &str,
    final_msg_id: &str,
    cancel: CancellationToken,
) -> CoreResult<ChatMessage> {
    let agent_folder = agent.folder.clone();
    let system_prompt = compose_system_prompt(agent, skills);
    let tool_schemas: Vec<ToolSchema> = if provider.supports_tools() {
        collect_tool_schemas(agent, skills, tools)
    } else {
        Vec::new()
    };

    // Build initial turn list from persisted history, trimmed to the window.
    let history = repo.load(&agent.id)?;
    let mut turns = history_to_turns(&history);
    trim_history(&mut turns, HISTORY_WINDOW);

    let mut run_totals = TokenUsage::default();
    let mut run_totals_seen = false;

    for iteration in 0..MAX_REACT_ITERATIONS {
        let (text, reasoning, tool_calls, finish_reason, iter_usage) = run_single_call(
            app,
            channel,
            provider.clone(),
            &tool_schemas,
            model_id,
            system_prompt.clone(),
            turns.clone(),
            cancel.clone(),
        )
        .await?;

        if let Some(u) = iter_usage {
            accumulate_usage(&mut run_totals, &u);
            run_totals_seen = true;
            tracing::debug!(
                provider = provider.id(),
                model = model_id,
                iteration,
                input_tokens = u.input_tokens,
                output_tokens = u.output_tokens,
                cached_input_tokens = u.cached_input_tokens,
                cache_creation_input_tokens = u.cache_creation_input_tokens,
                "chat iteration usage"
            );
        }

        // Record the assistant turn we just produced, either in-memory for
        // the next LLM call or (on final iteration) as the persisted message.
        if tool_calls.is_empty() {
            let assistant_msg = ChatMessage {
                id: final_msg_id.to_string(),
                role: MessageRole::Assistant,
                content: text.clone(),
                created_at: chrono::Utc::now().to_rfc3339(),
                tool_calls: Vec::new(),
                tool_results: Vec::new(),
                reasoning: reasoning.clone(),
            };
            if !assistant_msg.content.is_empty() || reasoning.is_some() {
                let _ = repo.append(&agent.id, &assistant_msg);
            }
            log_run_totals(
                provider.id(),
                model_id,
                iteration + 1,
                &run_totals,
                run_totals_seen,
            );
            return Ok(assistant_msg);
        }

        // Assistant asked for tool calls — keep the assistant turn in the
        // history (text + tool_calls) and append a user turn carrying the
        // results of each tool call, then loop.
        turns.push(ChatTurn {
            role: ChatRole::Assistant,
            content: text.clone(),
            tool_calls: tool_calls.clone(),
            tool_results: Vec::new(),
        });

        let assistant_persist = ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: text,
            created_at: chrono::Utc::now().to_rfc3339(),
            tool_calls: tool_calls.clone(),
            tool_results: Vec::new(),
            reasoning: reasoning.clone(),
        };
        // Persist below as a pair with the tool results — writing here
        // would leave orphaned tool_calls in history if the run is cancelled
        // or errors out before the tools finish.

        // Execute each tool and collect results. If a tool requires
        // human-in-the-loop approval, pause here, surface the preview to
        // the UI, and only run after the user clicks Freigeben.
        let mut results = Vec::with_capacity(tool_calls.len());
        for call in &tool_calls {
            if cancel.is_cancelled() {
                return Err(CoreError::Cancelled);
            }

            // ask_user is intercepted here — it's not a real tool but a
            // question/answer round-trip with the user. The runner pauses
            // until the user types an answer via `respond_to_question`.
            if call.name == "ask_user" {
                let question = call
                    .arguments
                    .get("question")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let question_id = Uuid::new_v4().to_string();
                let (tx, rx) = oneshot::channel::<String>();
                pending_questions
                    .lock()
                    .await
                    .insert(question_id.clone(), tx);
                let _ = app.emit(
                    channel,
                    RunEvent::AskUserRequest {
                        question_id: question_id.clone(),
                        tool_call_id: call.id.clone(),
                        question,
                    },
                );
                let answer = tokio::select! {
                    res = rx => res.unwrap_or_else(|_| "(keine Antwort)".to_string()),
                    _ = cancel.cancelled() => {
                        pending_questions.lock().await.remove(&question_id);
                        return Err(CoreError::Cancelled);
                    }
                };
                let _ = app.emit(
                    channel,
                    RunEvent::AskUserResolved {
                        question_id: question_id.clone(),
                        answer: answer.clone(),
                    },
                );
                let _ = app.emit(
                    channel,
                    RunEvent::ToolCallCompleted {
                        id: call.id.clone(),
                        content: answer.clone(),
                        is_error: false,
                    },
                );
                results.push(ToolResult {
                    tool_use_id: call.id.clone(),
                    content: answer,
                    is_error: false,
                });
                continue;
            }

            let folder = agent_folder.clone().ok_or_else(|| {
                CoreError::Llm("Agent hat keinen Ordner konfiguriert.".to_string())
            })?;
            let ctx = ToolContext {
                agent_id: agent.id.clone(),
                agent_folder: folder,
                app: app.clone(),
                channel: channel.to_string(),
                tool_call_id: call.id.clone(),
                cancel: cancel.clone(),
            };

            // Pre-flight: if the tool wants approval, gate it here. Skip the
            // gate entirely if the agent has its HITL toggle off — that's
            // the per-agent escape hatch for trusted, unattended runs.
            let tool = tools.get(&call.name);
            let approval = if agent.hitl_disabled {
                None
            } else {
                match &tool {
                    Ok(t) => t.requires_approval(&call.arguments, &ctx),
                    Err(_) => None,
                }
            };
            if let Some(preview) = approval {
                let hitl_id = Uuid::new_v4().to_string();
                let (tx, rx) = oneshot::channel::<HitlDecision>();
                {
                    let mut guard = pending_hitls.lock().await;
                    guard.insert(hitl_id.clone(), tx);
                }
                let _ = app.emit(
                    channel,
                    RunEvent::HitlRequest {
                        hitl_id: hitl_id.clone(),
                        tool_call_id: call.id.clone(),
                        tool_name: call.name.clone(),
                        preview,
                    },
                );

                let decision = tokio::select! {
                    res = rx => res.unwrap_or(HitlDecision::Reject {
                        reason: Some("Freigabe abgebrochen".into()),
                    }),
                    _ = cancel.cancelled() => {
                        // Drop the pending entry so it doesn't leak.
                        pending_hitls.lock().await.remove(&hitl_id);
                        return Err(CoreError::Cancelled);
                    }
                };

                let _ = app.emit(
                    channel,
                    RunEvent::HitlResolved {
                        hitl_id: hitl_id.clone(),
                        decision: decision.clone(),
                    },
                );

                if let HitlDecision::Reject { reason } = &decision {
                    let msg = reason
                        .clone()
                        .unwrap_or_else(|| "User rejected the change".to_string());
                    let _ = app.emit(
                        channel,
                        RunEvent::ToolCallCompleted {
                            id: call.id.clone(),
                            content: msg.clone(),
                            is_error: true,
                        },
                    );
                    results.push(ToolResult {
                        tool_use_id: call.id.clone(),
                        content: format!("REJECTED_BY_USER: {msg}"),
                        is_error: true,
                    });
                    continue;
                }
            }

            let (content, is_error) = match tool {
                Ok(tool) => match tool.execute(call.arguments.clone(), &ctx).await {
                    Ok(out) => (out.content, false),
                    Err(e) => (format!("tool error: {e}"), true),
                },
                Err(e) => (format!("unknown tool: {e}"), true),
            };
            let _ = app.emit(
                channel,
                RunEvent::ToolCallCompleted {
                    id: call.id.clone(),
                    content: content.clone(),
                    is_error,
                },
            );
            results.push(ToolResult {
                tool_use_id: call.id.clone(),
                content,
                is_error,
            });
        }

        let tool_turn = ChatTurn {
            role: ChatRole::User,
            content: String::new(),
            tool_calls: Vec::new(),
            tool_results: results.clone(),
        };
        turns.push(tool_turn);

        let tool_persist = ChatMessage {
            id: Uuid::new_v4().to_string(),
            role: MessageRole::Tool,
            content: String::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            tool_calls: Vec::new(),
            tool_results: results,
            reasoning: None,
        };
        let _ = repo.append(&agent.id, &assistant_persist);
        let _ = repo.append(&agent.id, &tool_persist);

        if finish_reason == FinishReason::Stop {
            // LLM said "stop" even though it also produced tool calls. Feed
            // the results back in any case and let it continue if needed.
        }
    }

    log_run_totals(
        provider.id(),
        model_id,
        MAX_REACT_ITERATIONS,
        &run_totals,
        run_totals_seen,
    );
    Err(CoreError::Llm(format!(
        "ReAct-Loop-Limit ({MAX_REACT_ITERATIONS}) erreicht."
    )))
}

fn accumulate_usage(acc: &mut TokenUsage, src: &TokenUsage) {
    acc.input_tokens = acc.input_tokens.saturating_add(src.input_tokens);
    acc.output_tokens = acc.output_tokens.saturating_add(src.output_tokens);
    if let Some(v) = src.cached_input_tokens {
        acc.cached_input_tokens = Some(acc.cached_input_tokens.unwrap_or(0).saturating_add(v));
    }
    if let Some(v) = src.cache_creation_input_tokens {
        acc.cache_creation_input_tokens = Some(
            acc.cache_creation_input_tokens
                .unwrap_or(0)
                .saturating_add(v),
        );
    }
}

fn log_run_totals(
    provider_id: &str,
    model_id: &str,
    iterations: u32,
    totals: &TokenUsage,
    seen: bool,
) {
    if !seen {
        return;
    }
    tracing::info!(
        provider = provider_id,
        model = model_id,
        iterations,
        input_tokens = totals.input_tokens,
        output_tokens = totals.output_tokens,
        cached_input_tokens = totals.cached_input_tokens,
        cache_creation_input_tokens = totals.cache_creation_input_tokens,
        "chat run usage"
    );
}

#[allow(clippy::too_many_arguments)]
async fn run_single_call(
    app: &AppHandle,
    channel: &str,
    provider: Arc<dyn crate::core::llm::LlmProvider>,
    tool_schemas: &[ToolSchema],
    model_id: &str,
    system_prompt: Option<String>,
    turns: Vec<ChatTurn>,
    cancel: CancellationToken,
) -> CoreResult<(
    String,
    Option<String>,
    Vec<ToolCall>,
    FinishReason,
    Option<TokenUsage>,
)> {
    let (tx, mut rx) = mpsc::channel::<LlmEvent>(64);

    let request = GenerateRequest {
        model_id: model_id.to_string(),
        system_prompt,
        turns,
        tools: tool_schemas.to_vec(),
        max_tokens: DEFAULT_MAX_TOKENS,
        temperature: None,
    };

    let provider_bg = provider.clone();
    let cancel_for_provider = cancel.clone();
    let gen_task =
        tokio::spawn(async move { provider_bg.generate(request, tx, cancel_for_provider).await });

    let mut buffer = String::new();
    let mut reasoning_buffer = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut finish_reason: Option<FinishReason> = None;
    let mut terminal_error: Option<(String, String)> = None;
    let mut usage: Option<TokenUsage> = None;

    while let Some(event) = rx.recv().await {
        match event {
            LlmEvent::TextDelta { text } => {
                buffer.push_str(&text);
                let _ = app.emit(channel, RunEvent::Delta { text });
            }
            LlmEvent::ReasoningDelta { text } => {
                reasoning_buffer.push_str(&text);
                let _ = app.emit(channel, RunEvent::ReasoningDelta { text });
            }
            LlmEvent::ToolCall(call) => {
                let _ = app.emit(
                    channel,
                    RunEvent::ToolCallStarted {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    },
                );
                tool_calls.push(call);
            }
            LlmEvent::Usage(u) => {
                usage = Some(u);
            }
            LlmEvent::Finish { reason } => {
                finish_reason = Some(reason);
            }
            LlmEvent::Error { code, message } => {
                terminal_error = Some((code, message));
            }
        }
    }

    let _ = gen_task.await;
    if let Some((code, message)) = terminal_error {
        return Err(CoreError::Llm(format!("{code}: {message}")));
    }
    let reasoning = if reasoning_buffer.trim().is_empty() {
        None
    } else {
        Some(reasoning_buffer)
    };
    Ok((
        buffer,
        reasoning,
        tool_calls,
        finish_reason.unwrap_or(FinishReason::Stop),
        usage,
    ))
}

fn compose_system_prompt(agent: &Agent, skills: &SkillRegistry) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    // Anchor temporal references like "today" / "yesterday" — the model
    // otherwise only knows its training cutoff and can't resolve them.
    let now = chrono::Local::now();
    parts.push(format!(
        "Today is {} ({}).",
        now.format("%Y-%m-%d"),
        now.format("%A"),
    ));
    if !agent.system_prompt.trim().is_empty() {
        parts.push(agent.system_prompt.trim().to_string());
    }
    if let Some(block) = skills_block(agent, skills) {
        parts.push(block);
    }
    if let Some(block) = attachments_block(agent) {
        parts.push(block);
    }
    parts.push("Respond in the language the user used.".to_string());
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

/// Render the agent's enabled skills as a compact "Available skills" list.
/// Only the description is included here; the body is loaded on demand via
/// the `read_skill` tool. This keeps the per-request input cost flat as more
/// skills are enabled.
fn skills_block(agent: &Agent, skills: &SkillRegistry) -> Option<String> {
    let entries: Vec<String> = agent
        .skills
        .iter()
        .filter_map(|name| skills.get(name))
        .map(|s| {
            let desc = s.description.trim();
            format!("- **{}** (id: `{}`) — {}", s.title, s.name, desc)
        })
        .collect();
    if entries.is_empty() {
        return None;
    }
    let mut block = String::from("## Available skills\n");
    block.push_str(
        "Each skill below is summarized; the full instructions live in its body. \
         If the user's request maps to one of these skills, call \
         `read_skill({ skillId: \"<id>\" })` to load the body — it will arrive as a tool \
         result and you must follow it before acting. If no skill is relevant, answer \
         directly without calling `read_skill`. Skills already loaded earlier in this \
         conversation stay in scope; don't reload them.\n\n",
    );
    block.push_str(&entries.join("\n"));
    Some(block)
}

/// Render the agent's attachments as a system-prompt hint. Phrased
/// conditionally so the model only acts on it when the relevant skill
/// triggers — preventing reflexive `read_docx` calls on unrelated chat.
fn attachments_block(agent: &Agent) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    if let Some(template) = agent.attachments.template_path.as_ref() {
        let display = pretty_attachment_path(agent, template);
        lines.push(format!(
            "- The user has chosen `{display}` as the template for the `document-from-template` skill. When that skill is invoked, skip the discovery step and call `read_docx` on this path directly."
        ));
    }
    if lines.is_empty() {
        return None;
    }
    let mut block = String::from("## Attachments\n");
    block.push_str(&lines.join("\n"));
    Some(block)
}

fn pretty_attachment_path(agent: &Agent, path: &std::path::Path) -> String {
    if let Some(folder) = agent.folder.as_ref() {
        if let Ok(stripped) = path.strip_prefix(folder) {
            return stripped.display().to_string();
        }
    }
    path.display().to_string()
}

fn collect_tool_schemas(
    agent: &Agent,
    skills: &SkillRegistry,
    tools: &ToolRegistry,
) -> Vec<ToolSchema> {
    let mut wanted: Vec<String> = Vec::new();
    // Whenever the agent has any skills configured, expose `read_skill` so
    // the model can pull in skill bodies on demand. The system prompt only
    // lists descriptions; bodies arrive as tool results when the model asks.
    if !agent.skills.is_empty() {
        wanted.push("read_skill".to_string());
    }
    for skill_name in &agent.skills {
        if let Some(skill) = skills.get(skill_name) {
            for t in &skill.tools {
                if !wanted.contains(t) {
                    wanted.push(t.clone());
                }
            }
        }
    }
    // Fan-out tools live outside the regular skill→tool plumbing — they are
    // gated by the agent's delegation profile, not by skill membership, so
    // that flipping the worker on/off doesn't require touching skills.
    if agent.delegation_profile.as_ref().is_some_and(|p| p.enabled)
        && !wanted.iter().any(|n| n == "delegate_into_xlsx_column")
    {
        wanted.push("delegate_into_xlsx_column".to_string());
    }
    tools.schemas_for(&wanted)
}

fn history_to_turns(history: &[ChatMessage]) -> Vec<ChatTurn> {
    let mut turns = Vec::new();
    for m in history {
        let role = match m.role {
            MessageRole::User => ChatRole::User,
            MessageRole::Assistant => ChatRole::Assistant,
            MessageRole::Tool => ChatRole::User,
            MessageRole::System => continue,
        };
        turns.push(ChatTurn {
            role,
            content: m.content.clone(),
            tool_calls: m.tool_calls.clone(),
            tool_results: m.tool_results.clone(),
        });
    }
    turns
}

/// Trim the turn list to at most `window` turns *and* heal the leading
/// boundary. A naive `drain` can land mid-pair, leaving an orphan
/// tool_result whose matching tool_use was dropped — Anthropic rejects that
/// with a 400, and OpenAI is similarly strict. We also drop any leading
/// assistant turn because Anthropic requires the first message in a request
/// to be a user turn.
///
/// The result may be shorter than `window`; it is never longer.
fn trim_history(turns: &mut Vec<ChatTurn>, window: usize) {
    if turns.len() > window {
        turns.drain(0..turns.len() - window);
    }
    while turns.first().is_some_and(|t| {
        t.role != ChatRole::User || !t.tool_results.is_empty() || !t.tool_calls.is_empty()
    }) {
        turns.remove(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(content: &str) -> ChatTurn {
        ChatTurn {
            role: ChatRole::User,
            content: content.to_string(),
            tool_calls: Vec::new(),
            tool_results: Vec::new(),
        }
    }

    fn assistant_text(content: &str) -> ChatTurn {
        ChatTurn {
            role: ChatRole::Assistant,
            content: content.to_string(),
            tool_calls: Vec::new(),
            tool_results: Vec::new(),
        }
    }

    fn assistant_tool_call(id: &str, name: &str) -> ChatTurn {
        ChatTurn {
            role: ChatRole::Assistant,
            content: String::new(),
            tool_calls: vec![ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments: serde_json::json!({}),
            }],
            tool_results: Vec::new(),
        }
    }

    fn tool_result(use_id: &str, content: &str) -> ChatTurn {
        ChatTurn {
            role: ChatRole::User,
            content: String::new(),
            tool_calls: Vec::new(),
            tool_results: vec![ToolResult {
                tool_use_id: use_id.to_string(),
                content: content.to_string(),
                is_error: false,
            }],
        }
    }

    #[test]
    fn trim_history_drops_leading_orphan_tool_result() {
        // Window cuts off the assistant tool_use; the matching tool_result
        // would land at position 0 and Anthropic would reject it.
        let mut turns = vec![
            tool_result("call_1", "result"),
            assistant_text("ok"),
            user("next question"),
        ];
        trim_history(&mut turns, 10);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].content, "next question");
    }

    #[test]
    fn trim_history_drops_leading_assistant_turn() {
        // Window starts on an assistant text turn — first message must be
        // user, so drop it.
        let mut turns = vec![assistant_text("answer"), user("follow up")];
        trim_history(&mut turns, 10);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].content, "follow up");
    }

    #[test]
    fn trim_history_keeps_intact_pair_at_boundary() {
        // The trimmed window starts cleanly with a user turn, then has an
        // intact tool_use → tool_result pair. Nothing should be dropped.
        let mut turns = vec![
            user("question"),
            assistant_tool_call("call_1", "list_folder"),
            tool_result("call_1", "files: …"),
            assistant_text("here's what I found"),
            user("thanks"),
        ];
        let before = turns.len();
        trim_history(&mut turns, 10);
        assert_eq!(turns.len(), before);
    }

    #[test]
    fn trim_history_walks_through_chain_of_orphans() {
        // The window ate two pairs midway, leaving alternating orphan
        // tool_results and assistant turns until a clean user turn appears.
        let mut turns = vec![
            tool_result("call_1", "r1"),
            assistant_text("text"),
            tool_result("call_2", "r2"),
            assistant_text("more text"),
            user("real question"),
        ];
        trim_history(&mut turns, 10);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].content, "real question");
    }

    #[test]
    fn trim_history_respects_window_size() {
        // 30 turns, window 5: drain to last 5, then the heal loop only
        // touches the leading orphan if any.
        let mut turns: Vec<ChatTurn> = (0..30).map(|i| user(&format!("u{i}"))).collect();
        trim_history(&mut turns, 5);
        assert_eq!(turns.len(), 5);
        assert_eq!(turns[0].content, "u25");
        assert_eq!(turns[4].content, "u29");
    }
}
