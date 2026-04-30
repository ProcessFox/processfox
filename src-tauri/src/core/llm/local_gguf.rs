//! Local GGUF inference provider backed by llama.cpp via `llama-cpp-2`.
//!
//! One loaded model at a time; switching models triggers an unload + reload.
//! On macOS aarch64 the crate auto-enables Metal via its own target-spec.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::openai::OpenAIChatTemplateParams;
use llama_cpp_2::sampling::LlamaSampler;
use serde_json::{json, Value as JsonValue};
use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use super::json_cleanup::extract_json_value;
use super::{ChatRole, FinishReason, GenerateRequest, LlmEvent, LlmProvider, TokenUsage, ToolCall};
use crate::core::error::{CoreError, CoreResult};
use crate::core::tool::ToolSchema;

pub const PROVIDER_ID: &str = "local";

const DEFAULT_CTX: u32 = 4096;
const N_GPU_LAYERS_ALL: u32 = 1000;
const IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(60);

/// Process-wide llama backend. `LlamaBackend::init()` must only be called once
/// for the lifetime of the process; we lazily initialize it the first time a
/// local model is requested.
fn shared_backend() -> CoreResult<&'static LlamaBackend> {
    static BACKEND: OnceLock<LlamaBackend> = OnceLock::new();
    if let Some(b) = BACKEND.get() {
        return Ok(b);
    }
    let backend =
        LlamaBackend::init().map_err(|e| CoreError::Llm(format!("llama backend init: {e}")))?;
    Ok(BACKEND.get_or_init(|| backend))
}

struct Loaded {
    filename: String,
    model: Arc<LlamaModel>,
    template: LlamaChatTemplate,
}

impl std::fmt::Debug for Loaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Loaded")
            .field("filename", &self.filename)
            .finish()
    }
}

#[derive(Debug)]
struct LoadedSlot {
    loaded: Loaded,
    last_used: Instant,
}

#[derive(Debug)]
pub struct LocalGgufProvider {
    models_dir: PathBuf,
    loaded: Arc<Mutex<Option<LoadedSlot>>>,
    watcher_started: AtomicBool,
}

impl LocalGgufProvider {
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            loaded: Arc::new(Mutex::new(None)),
            watcher_started: AtomicBool::new(false),
        }
    }

    pub async fn unload(&self) {
        *self.loaded.lock().await = None;
    }

    /// Spawns the idle-unload watcher on first use. Held models eat several GB
    /// of RAM, so we drop them after `IDLE_TIMEOUT` of no `ensure_loaded`
    /// activity. Lazy spawn guarantees we're inside a tokio runtime.
    fn ensure_idle_watcher(&self) {
        if self
            .watcher_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let loaded = Arc::clone(&self.loaded);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(IDLE_CHECK_INTERVAL).await;
                let mut guard = loaded.lock().await;
                let should_drop = guard
                    .as_ref()
                    .map(|s| s.last_used.elapsed() >= IDLE_TIMEOUT)
                    .unwrap_or(false);
                if should_drop {
                    if let Some(slot) = guard.as_ref() {
                        tracing::info!(
                            filename = slot.loaded.filename,
                            "unloading idle local GGUF model"
                        );
                    }
                    *guard = None;
                }
            }
        });
    }

    /// Get the requested model, loading it if it's not already resident.
    /// Switching to a different `filename` triggers an unload of the
    /// previous model first. Touches `last_used` on every call so the idle
    /// watcher only drops models that have actually gone quiet.
    async fn ensure_loaded(&self, filename: &str) -> CoreResult<Loaded> {
        self.ensure_idle_watcher();
        {
            let mut guard = self.loaded.lock().await;
            if let Some(current) = guard.as_mut() {
                if current.loaded.filename == filename {
                    current.last_used = Instant::now();
                    return Ok(Loaded {
                        filename: current.loaded.filename.clone(),
                        model: Arc::clone(&current.loaded.model),
                        template: current.loaded.template.clone(),
                    });
                }
            }
        }

        // Drop any previous model before loading a new one.
        {
            let mut guard = self.loaded.lock().await;
            *guard = None;
        }

        let backend = shared_backend()?;
        let path = self.models_dir.join(filename);
        if !path.exists() {
            return Err(CoreError::PathInvalid(format!(
                "Modell-Datei fehlt: {}",
                path.display()
            )));
        }
        tracing::info!(filename, path = %path.display(), "loading local GGUF model");

        let filename_owned = filename.to_string();
        let path_owned = path.clone();
        let loaded: Loaded = tokio::task::spawn_blocking(move || -> CoreResult<Loaded> {
            let model_params = LlamaModelParams::default().with_n_gpu_layers(N_GPU_LAYERS_ALL);
            let model = LlamaModel::load_from_file(backend, &path_owned, &model_params)
                .map_err(|e| CoreError::Llm(format!("Modell-Load fehlgeschlagen: {e}")))?;
            let template = model
                .chat_template(None)
                .map_err(|e| CoreError::Llm(format!("Kein Chat-Template im GGUF: {e}")))?;
            Ok(Loaded {
                filename: filename_owned,
                model: Arc::new(model),
                template,
            })
        })
        .await
        .map_err(|e| CoreError::Llm(format!("Load-Task abgebrochen: {e}")))??;

        let mut guard = self.loaded.lock().await;
        *guard = Some(LoadedSlot {
            loaded: Loaded {
                filename: loaded.filename.clone(),
                model: Arc::clone(&loaded.model),
                template: loaded.template.clone(),
            },
            last_used: Instant::now(),
        });
        Ok(loaded)
    }
}

/// Convert our turn list into the OpenAI-compatible messages array that
/// `apply_chat_template_oaicompat` consumes. Same shape as our cloud
/// providers — assistant turns with `tool_calls`, tool-result turns with
/// `role: "tool"`.
fn turns_to_openai_messages(request: &GenerateRequest) -> Vec<JsonValue> {
    let mut out: Vec<JsonValue> = Vec::new();
    if let Some(sys) = request.system_prompt.as_deref() {
        if !sys.trim().is_empty() {
            out.push(json!({ "role": "system", "content": sys }));
        }
    }
    for turn in &request.turns {
        if turn.role == ChatRole::System {
            continue;
        }
        if !turn.tool_results.is_empty() {
            for tr in &turn.tool_results {
                out.push(json!({
                    "role": "tool",
                    "tool_call_id": tr.tool_use_id,
                    "content": tr.content,
                }));
            }
            if turn.content.is_empty() && turn.tool_calls.is_empty() {
                continue;
            }
        }
        let mut msg = json!({ "role": role_str(turn.role) });
        let obj = msg.as_object_mut().unwrap();
        obj.insert("content".into(), JsonValue::String(turn.content.clone()));
        if !turn.tool_calls.is_empty() && turn.role == ChatRole::Assistant {
            let calls: Vec<JsonValue> = turn
                .tool_calls
                .iter()
                .map(|tc| {
                    json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": serde_json::to_string(&tc.arguments)
                                .unwrap_or_else(|_| "{}".to_string()),
                        }
                    })
                })
                .collect();
            obj.insert("tool_calls".into(), JsonValue::Array(calls));
        }
        out.push(msg);
    }
    out
}

fn role_str(role: ChatRole) -> &'static str {
    match role {
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::System => "system",
    }
}

fn tools_to_openai_array(tools: &[ToolSchema]) -> Vec<JsonValue> {
    tools
        .iter()
        .map(|t| {
            json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema,
                }
            })
        })
        .collect()
}

#[async_trait]
impl LlmProvider for LocalGgufProvider {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    fn supports_tools(&self) -> bool {
        true
    }

    async fn validate(&self) -> CoreResult<()> {
        if !self.models_dir.exists() {
            return Err(CoreError::Llm(format!(
                "Modell-Ordner existiert nicht: {}",
                self.models_dir.display()
            )));
        }
        Ok(())
    }

    async fn generate(
        &self,
        request: GenerateRequest,
        sink: mpsc::Sender<LlmEvent>,
        cancel: CancellationToken,
    ) -> CoreResult<()> {
        if request.model_id.trim().is_empty() {
            return Err(CoreError::Llm(
                "Kein lokales Modell ausgewählt.".to_string(),
            ));
        }

        let loaded = tokio::select! {
            r = self.ensure_loaded(&request.model_id) => r?,
            _ = cancel.cancelled() => {
                let _ = sink
                    .send(LlmEvent::Finish { reason: FinishReason::Cancelled })
                    .await;
                return Err(CoreError::Cancelled);
            }
        };

        // Render prompt using the model's own chat template, with our tool
        // schemas folded in. llama.cpp handles model-specific tool format
        // (Llama 3, Gemma 4, Qwen, …) internally.
        let messages = turns_to_openai_messages(&request);
        let messages_json = serde_json::to_string(&messages).unwrap_or_else(|_| "[]".to_string());
        let tools_json_owned = if request.tools.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&tools_to_openai_array(&request.tools))
                    .unwrap_or_else(|_| "[]".to_string()),
            )
        };

        let max_tokens = request.max_tokens;
        let temperature = request.temperature.unwrap_or(0.7);

        let backend = shared_backend()?;
        let model_arc = Arc::clone(&loaded.model);
        let template = loaded.template.clone();
        let cancel_blocking = cancel.clone();
        let sink_blocking = sink.clone();

        let join =
            tokio::task::spawn_blocking(move || -> CoreResult<(FinishReason, TokenUsage)> {
                run_generation(
                    backend,
                    &model_arc,
                    &template,
                    &messages_json,
                    tools_json_owned.as_deref(),
                    max_tokens,
                    temperature,
                    cancel_blocking,
                    sink_blocking,
                )
            })
            .await
            .map_err(|e| CoreError::Llm(format!("Generation-Task: {e}")))?;

        match join {
            Ok((reason, usage)) => {
                let _ = sink.send(LlmEvent::Usage(usage)).await;
                let _ = sink.send(LlmEvent::Finish { reason }).await;
                Ok(())
            }
            Err(e) => {
                if matches!(e, CoreError::Cancelled) {
                    let _ = sink
                        .send(LlmEvent::Finish {
                            reason: FinishReason::Cancelled,
                        })
                        .await;
                    return Err(e);
                }
                let _ = sink
                    .send(LlmEvent::Error {
                        code: "llm_error".into(),
                        message: e.to_string(),
                    })
                    .await;
                Err(e)
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_generation(
    backend: &'static LlamaBackend,
    model: &LlamaModel,
    template: &LlamaChatTemplate,
    messages_json: &str,
    tools_json: Option<&str>,
    max_tokens: u32,
    temperature: f32,
    cancel: CancellationToken,
    sink: mpsc::Sender<LlmEvent>,
) -> CoreResult<(FinishReason, TokenUsage)> {
    let params = OpenAIChatTemplateParams {
        messages_json,
        tools_json,
        tool_choice: None,
        json_schema: None,
        grammar: None,
        // "auto" tells llama.cpp to detect the model-specific reasoning
        // wrapper (Gemma 4 channel tags, DeepSeek <think>, etc.) and split
        // it into the separate `reasoning_content` delta field instead of
        // leaking it into the visible answer.
        reasoning_format: Some("auto"),
        chat_template_kwargs: None,
        add_generation_prompt: true,
        use_jinja: true,
        parallel_tool_calls: false,
        enable_thinking: true,
        add_bos: false,
        add_eos: false,
        parse_tool_calls: tools_json.is_some(),
    };

    let chat_result = model
        .apply_chat_template_oaicompat(template, &params)
        .map_err(|e| CoreError::Llm(format!("apply_chat_template_oaicompat: {e}")))?;

    let tokens = model
        .str_to_token(&chat_result.prompt, AddBos::Always)
        .map_err(|e| CoreError::Llm(format!("Tokenisierung: {e}")))?;

    let n_ctx = (tokens.len() as u32 + max_tokens).max(DEFAULT_CTX);
    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(n_ctx))
        .with_n_batch(n_ctx);
    let mut ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| CoreError::Llm(format!("Context-Init: {e}")))?;

    // Feed prompt tokens.
    let mut batch = LlamaBatch::new(n_ctx as usize, 1);
    let last_index = tokens.len().saturating_sub(1) as i32;
    for (i, token) in (0_i32..).zip(tokens.iter().copied()) {
        let is_last = i == last_index;
        batch
            .add(token, i, &[0], is_last)
            .map_err(|e| CoreError::Llm(format!("Batch-Add: {e}")))?;
    }
    ctx.decode(&mut batch)
        .map_err(|e| CoreError::Llm(format!("Decode: {e}")))?;

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::dist(0),
        if temperature > 0.0 {
            LlamaSampler::temp(temperature)
        } else {
            LlamaSampler::greedy()
        },
    ]);

    // Streaming parser: handles model-specific tool-call markup and emits
    // OpenAI-compatible JSON deltas as tokens flow in.
    let mut parser = chat_result
        .streaming_state_oaicompat()
        .map_err(|e| CoreError::Llm(format!("Stream-Parser-Init: {e}")))?;

    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut n_cur = batch.n_tokens();
    let max_total = n_cur + max_tokens as i32;
    let mut pending_tool_calls: HashMap<u32, PendingToolCall> = HashMap::new();
    let mut finish = FinishReason::Stop;

    while n_cur < max_total {
        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if model.is_eog_token(token) {
            break;
        }
        let bytes = model
            .token_to_piece_bytes(token, 64, false, None)
            .map_err(|e| CoreError::Llm(format!("Token-Decode: {e}")))?;
        let mut piece = String::with_capacity(32);
        let _ = decoder.decode_to_string(&bytes, &mut piece, false);

        // Pass the new chunk through the chat-template-aware parser. It may
        // emit zero, one, or several OpenAI-style delta chunks.
        let deltas = parser
            .update(&piece, true)
            .map_err(|e| CoreError::Llm(format!("Stream-Parser: {e}")))?;
        for delta_json in deltas {
            forward_delta(&delta_json, &sink, &mut pending_tool_calls)?;
        }

        batch.clear();
        batch
            .add(token, n_cur, &[0], true)
            .map_err(|e| CoreError::Llm(format!("Batch-Add: {e}")))?;
        n_cur += 1;
        ctx.decode(&mut batch)
            .map_err(|e| CoreError::Llm(format!("Decode: {e}")))?;
    }

    if n_cur >= max_total {
        finish = FinishReason::MaxTokens;
    }

    // Final flush: tells the parser the stream is complete; any in-flight
    // tool calls get materialized.
    let final_deltas = parser
        .update("", false)
        .map_err(|e| CoreError::Llm(format!("Stream-Parser-Flush: {e}")))?;
    for delta_json in final_deltas {
        forward_delta(&delta_json, &sink, &mut pending_tool_calls)?;
    }
    flush_pending(&mut pending_tool_calls, &sink);

    if !pending_tool_calls.is_empty() || tool_calls_were_emitted(&sink) {
        // ToolUse takes precedence as the finish reason if the LLM ends with
        // pending tool calls. We don't have a strong signal beyond what
        // we've forwarded though; let the runner decide based on the events.
        finish = FinishReason::ToolUse;
    }

    let usage = TokenUsage {
        input_tokens: tokens.len() as u32,
        output_tokens: (n_cur - tokens.len() as i32).max(0) as u32,
        cached_input_tokens: None,
        cache_creation_input_tokens: None,
    };
    Ok((finish, usage))
}

/// In-progress tool call assembled from streamed deltas.
#[derive(Default, Debug)]
struct PendingToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments_raw: String,
}

/// Parse one OpenAI-compat delta JSON and emit the right `LlmEvent` to the
/// chat runner. Tool calls arrive in fragments and get accumulated by index;
/// `flush_pending` emits the assembled `LlmEvent::ToolCall`s on stream end.
fn forward_delta(
    delta_json: &str,
    sink: &mpsc::Sender<LlmEvent>,
    pending: &mut HashMap<u32, PendingToolCall>,
) -> CoreResult<()> {
    let value: JsonValue = serde_json::from_str(delta_json)
        .map_err(|e| CoreError::Llm(format!("Delta-Parse: {e}")))?;
    if let Some(content) = value.get("content").and_then(|v| v.as_str()) {
        if !content.is_empty() {
            let _ = sink.blocking_send(LlmEvent::TextDelta {
                text: content.to_string(),
            });
        }
    }
    if let Some(reasoning) = value.get("reasoning_content").and_then(|v| v.as_str()) {
        if !reasoning.is_empty() {
            let _ = sink.blocking_send(LlmEvent::ReasoningDelta {
                text: reasoning.to_string(),
            });
        }
    }
    if let Some(arr) = value.get("tool_calls").and_then(|v| v.as_array()) {
        for tc in arr {
            let index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let entry = pending.entry(index).or_default();
            if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                if entry.id.is_none() && !id.is_empty() {
                    entry.id = Some(id.to_string());
                }
            }
            if let Some(func) = tc.get("function") {
                if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                    if entry.name.is_none() && !name.is_empty() {
                        entry.name = Some(name.to_string());
                    }
                }
                if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                    entry.arguments_raw.push_str(args);
                }
            }
        }
    }
    Ok(())
}

fn flush_pending(pending: &mut HashMap<u32, PendingToolCall>, sink: &mpsc::Sender<LlmEvent>) {
    if pending.is_empty() {
        return;
    }
    let mut entries: Vec<(u32, PendingToolCall)> = pending.drain().collect();
    entries.sort_by_key(|(i, _)| *i);
    for (_, pt) in entries {
        let id = pt
            .id
            .unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4()));
        let Some(name) = pt.name else { continue };
        let arguments = if pt.arguments_raw.trim().is_empty() {
            JsonValue::Object(Default::default())
        } else {
            extract_json_value(&pt.arguments_raw)
        };
        let _ = sink.blocking_send(LlmEvent::ToolCall(ToolCall {
            id,
            name,
            arguments,
        }));
    }
}

/// Probe: did we forward a ToolCall already? We can't introspect the
/// channel, so this is a simple heuristic placeholder. The chat runner
/// looks at events, not at this finish reason directly, so a wrong default
/// just means a slightly less precise reason field.
fn tool_calls_were_emitted(_sink: &mpsc::Sender<LlmEvent>) -> bool {
    false
}
