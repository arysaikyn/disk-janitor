//! HTTP `ChatClient` (feature `llm`) — async-openai over reqwest, non-streaming.
//!
//! SPEC Part B §3.7: the model call is non-streaming (`stream: false`); the sans-IO
//! trait `ChatClient::complete` is SYNC, so the async runtime is confined to this
//! module: `HttpChatClient` owns a `tokio::runtime::Runtime` and `block_on`s each
//! call on the agent thread (SPEC §2.1 — async never leaks into the sans-IO loop).
//!
//! Provider quirks handled here (research note 07 §providers):
//! - **DeepSeek**: non-streaming + no thinking-mode params on the wire sidesteps the
//!   `reasoning_content` echo rule entirely (`cfg.thinking` stays a caller-side knob).
//! - **Ollama**: older daemons omit `tool_call.id`. The typed response requires `id`,
//!   so the whole body fails to deserialize; we catch `OpenAIError::JSONDeserialize`
//!   and re-parse the body leniently, minting synthetic ids `call_<idx>`.
//! - **Usage**: provider-reported counts are ground truth for billing. `cached_tokens`
//!   comes from `prompt_tokens_details.cached_tokens` when present, else 0. A missing
//!   usage chunk maps to zeroed `Usage` — estimates are never billed (note 07 §cost).
//! - **Retry**: async-openai 0.41.3's always-on `OpenAIRetryLayer` (executor default)
//!   retries 429 (rate-limit, not insufficient-quota), 5xx, and connect errors up to
//!   3 times, honoring `Retry-After` and falling back to exponential 100ms×2ⁿ capped
//!   8s (src/middleware/retry/openai.rs). No outer loop is layered on top: the typed
//!   error surface exposes no response headers, so an outer loop could not honor
//!   `Retry-After` any better than the built-in layer already does.

use async_openai::Client as OpenAIClient;
use async_openai::config::OpenAIConfig;
use async_openai::error::OpenAIError;
use async_openai::types::chat::{
    ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestToolMessage,
    ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent, ChatCompletionTool, ChatCompletionTools,
    CreateChatCompletionRequest, CreateChatCompletionResponse, FunctionCall, FunctionObject,
};
use serde::Deserialize;
use serde_json::json;

use crate::agent::{AgentError, ChatClient, ModelReply, Msg, Role, ToolCall, ToolDef, Usage};
use crate::config::Config;

/// The OpenAI-compatible chat client (non-streaming chat-completions).
pub struct HttpChatClient {
    client: OpenAIClient<OpenAIConfig>,
    model: String,
    /// Async confined to the agent thread (SPEC §2.1): `ChatClient::complete` is sync,
    /// so each call is `block_on`'d on this runtime.
    rt: tokio::runtime::Runtime,
}

impl HttpChatClient {
    /// `cfg.endpoint` is the OpenAI-compatible base URL (no trailing slash), e.g.
    /// `https://api.deepseek.com/v1`, `https://openrouter.ai/api/v1`,
    /// `http://localhost:11434/v1` (Ollama).
    pub fn new(cfg: Config) -> Result<Self, AgentError> {
        let api_key = cfg.api_key.ok_or_else(|| {
            AgentError::Model("no API key — set DISK_JANITOR_API_KEY or config.toml".into())
        })?;
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| AgentError::Model(format!("tokio runtime: {e}")))?;
        // E2: defensive trim — `Config::load` already normalizes, but a hand-built
        // Config may carry a trailing '/' (async-openai would join `//chat/completions`).
        let openai_cfg = OpenAIConfig::new()
            .with_api_base(crate::config::trim_endpoint(&cfg.endpoint))
            .with_api_key(api_key);
        Ok(Self {
            client: OpenAIClient::with_config(openai_cfg),
            model: cfg.model,
            rt,
        })
    }

    /// One non-streaming round trip (no retry; retries live in `complete`). Errors
    /// stay typed `OpenAIError` so the retry predicate can inspect status codes.
    async fn call_once(&self, msgs: &[Msg], tools: &[ToolDef]) -> Result<ModelReply, OpenAIError> {
        let request = build_request(&self.model, msgs, tools);
        match self.client.chat().create(request).await {
            Ok(resp) => Ok(reply_from_response(resp)),
            Err(OpenAIError::JSONDeserialize(_, body)) => {
                // Ollama id-less tool calls: the typed response requires `id`, so the
                // body fails to deserialize wholesale — re-parse leniently and mint
                // synthetic ids (note 07 §providers/Ollama).
                reply_from_lenient_body(&body)
            }
            Err(e) => Err(e),
        }
    }
}

impl ChatClient for HttpChatClient {
    fn complete(&self, msgs: &[Msg], tools: &[ToolDef]) -> Result<ModelReply, AgentError> {
        // Retry lives inside async-openai's always-on `OpenAIRetryLayer` (429/5xx/
        // connect, up to 3 retries, Retry-After honored — module docs above).
        // `block_on` keeps the async runtime confined to the agent thread (§2.1).
        self.rt
            .block_on(self.call_once(msgs, tools))
            .map_err(|e| AgentError::Model(e.to_string()))
    }
}

/// Trajectory + tool defs → one `CreateChatCompletionRequest` (`stream: false`).
fn build_request(model: &str, msgs: &[Msg], tools: &[ToolDef]) -> CreateChatCompletionRequest {
    let messages = msgs.iter().map(to_wire_msg).collect();
    let tools = if tools.is_empty() {
        None
    } else {
        Some(
            tools
                .iter()
                .map(|t| {
                    ChatCompletionTools::Function(ChatCompletionTool {
                        function: FunctionObject {
                            name: t.name.clone(),
                            description: Some(t.description.clone()),
                            parameters: Some(t.parameters.clone()),
                            strict: None,
                        },
                    })
                })
                .collect(),
        )
    };
    CreateChatCompletionRequest {
        model: model.to_string(),
        messages,
        tools,
        stream: Some(false), // §3.7: non-streaming agent loop
        ..Default::default()
    }
}

fn to_wire_msg(m: &Msg) -> ChatCompletionRequestMessage {
    match m.role {
        Role::System => ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
            content: ChatCompletionRequestSystemMessageContent::Text(m.content.clone()),
            ..Default::default()
        }),
        Role::User => ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
            content: ChatCompletionRequestUserMessageContent::Text(m.content.clone()),
            ..Default::default()
        }),
        Role::Assistant => {
            // E5: an assistant message with BOTH content and tool_calls null is
            // invalid for strict providers (resume-safety) — send `""` when there is
            // nothing else to say. Tool-call echoes still omit content entirely.
            let content = if m.content.is_empty() {
                if m.tool_calls.is_empty() {
                    Some(ChatCompletionRequestAssistantMessageContent::Text(
                        String::new(),
                    ))
                } else {
                    None // never send `content: ""` on tool-call echo messages
                }
            } else {
                Some(ChatCompletionRequestAssistantMessageContent::Text(
                    m.content.clone(),
                ))
            };
            // The OpenAI protocol requires echoing the assistant's calls verbatim.
            let tool_calls = if m.tool_calls.is_empty() {
                None
            } else {
                Some(
                    m.tool_calls
                        .iter()
                        .map(|c| {
                            ChatCompletionMessageToolCalls::Function(
                                ChatCompletionMessageToolCall {
                                    id: c.id.clone(),
                                    function: FunctionCall {
                                        name: c.name.clone(),
                                        arguments: c.arguments.to_string(),
                                    },
                                },
                            )
                        })
                        .collect(),
                )
            };
            ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
                content,
                tool_calls,
                ..Default::default()
            })
        }
        Role::Tool => ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
            content: ChatCompletionRequestToolMessageContent::Text(m.content.clone()),
            tool_call_id: m.tool_call_id.clone().unwrap_or_default(),
        }),
    }
}

/// Typed response → `ModelReply`. Tool-call-id presence is guaranteed by the typed
/// shape; id-less responses never get here (they fail typed deserialization and are
/// re-parsed by `reply_from_lenient_body` instead).
fn reply_from_response(resp: CreateChatCompletionResponse) -> ModelReply {
    let choice = resp.choices.into_iter().next();
    let content = choice.as_ref().and_then(|c| c.message.content.clone());
    let tool_calls = choice
        .map(|c| {
            c.message
                .tool_calls
                .unwrap_or_default()
                .into_iter()
                .map(|tc| match tc {
                    ChatCompletionMessageToolCalls::Function(f) => ToolCall {
                        id: f.id,
                        name: f.function.name,
                        arguments: parse_arguments(&f.function.arguments),
                    },
                    ChatCompletionMessageToolCalls::Custom(c) => ToolCall {
                        id: c.id,
                        name: c.custom_tool.name,
                        arguments: parse_arguments(&c.custom_tool.input),
                    },
                })
                .collect()
        })
        .unwrap_or_default();
    let usage = resp
        .usage
        .map(|u| Usage {
            prompt_tokens: u.prompt_tokens as u64,
            completion_tokens: u.completion_tokens as u64,
            cached_tokens: u
                .prompt_tokens_details
                .as_ref()
                .and_then(|d| d.cached_tokens)
                .unwrap_or(0) as u64,
        })
        .unwrap_or_default(); // missing usage chunk → zeros, never an estimate (note 07)
    ModelReply {
        content,
        tool_calls,
        usage,
    }
}

/// The wire carries tool arguments as a JSON *string*; the loop validates them
/// (`CallTools` → tool impls parse the Value). Malformed JSON is preserved raw as
/// `{"_raw": <string>}` so the ToolFailLoop retry accounting sees the exact attempt
/// (§3.7; note 07 §arguments).
fn parse_arguments(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|_| json!({ "_raw": s }))
}

// --- lenient re-parse for Ollama id-less tool calls --------------------------------

#[derive(Deserialize)]
struct LenientResponse {
    choices: Vec<LenientChoice>,
    usage: Option<LenientUsage>,
}

#[derive(Deserialize)]
struct LenientChoice {
    message: LenientMessage,
}

#[derive(Deserialize)]
struct LenientMessage {
    content: Option<String>,
    tool_calls: Option<Vec<LenientToolCall>>,
}

#[derive(Deserialize)]
struct LenientToolCall {
    #[serde(default)]
    id: Option<String>,
    function: Option<LenientFunction>,
    #[serde(default)]
    custom_tool: Option<LenientFunction>,
}

#[derive(Deserialize)]
struct LenientFunction {
    name: String,
    #[serde(default)]
    arguments: String,
    #[serde(default)]
    input: String,
}

#[derive(Deserialize)]
struct LenientUsage {
    #[serde(default)]
    prompt_tokens: Option<u32>,
    #[serde(default)]
    completion_tokens: Option<u32>,
    prompt_tokens_details: Option<LenientPromptDetails>,
}

#[derive(Deserialize)]
struct LenientPromptDetails {
    #[serde(default)]
    cached_tokens: Option<u32>,
}

fn reply_from_lenient_body(body: &str) -> Result<ModelReply, OpenAIError> {
    let resp: LenientResponse = serde_json::from_str(body)
        .map_err(|e| OpenAIError::JSONDeserialize(e, body.to_string()))?;
    let choice = resp.choices.into_iter().next();
    let content = choice.as_ref().and_then(|c| c.message.content.clone());
    let tool_calls = choice
        .map(|c| {
            c.message
                .tool_calls
                .unwrap_or_default()
                .into_iter()
                .enumerate()
                .map(|(idx, tc)| {
                    let (name, args) = if let Some(f) = tc.function {
                        (f.name, f.arguments)
                    } else if let Some(c) = tc.custom_tool {
                        (c.name, c.input)
                    } else {
                        (String::new(), String::new())
                    };
                    ToolCall {
                        // Ollama older daemons omit `id` — mint a synthetic one
                        // (note 07 §providers/Ollama).
                        id: tc.id.unwrap_or_else(|| format!("call_{idx}")),
                        name,
                        arguments: parse_arguments(&args),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let usage = resp
        .usage
        .map(|u| Usage {
            prompt_tokens: u.prompt_tokens.unwrap_or(0) as u64,
            completion_tokens: u.completion_tokens.unwrap_or(0) as u64,
            cached_tokens: u
                .prompt_tokens_details
                .and_then(|d| d.cached_tokens)
                .unwrap_or(0) as u64,
        })
        .unwrap_or_default();
    Ok(ModelReply {
        content,
        tool_calls,
        usage,
    })
}
