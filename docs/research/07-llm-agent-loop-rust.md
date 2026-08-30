# LLM Agent Loop in Rust (OpenAI-compatible chat-completions) — Research Note

How to build the `agent/` module: HTTP + SSE, tool-calling loop, token/cost accounting,
context management, budget & stop conditions, privacy redaction layer · researched 2026-08-30

## Findings

### Transport: reqwest 0.13 + rustls is now the default story

reqwest 0.13.4 (2026-05) made `rustls` the **default** TLS backend (crypto provider =
aws-lc-rs; `rustls-no-provider` feature if you want ring/your own; roots via
`rustls-platform-verifier`). Feature was renamed `rustls` (from `rustls-tls`), and
`query`/`form` became opt-in features — enable `json` explicitly (it remains default-on
for the client builder API; verify at build time). 0.12.23+ also gained a built-in
`ClientBuilder::retry(policy)` (`reqwest::retry::Builder`). For Disk Janitor: plain
`reqwest` with default features already gives rustls + platform roots on macOS; no
native-tls/OpenSSL anywhere. async-openai 0.41.3 carries the same story: `default =
["rustls"]`, `rustls = ["dep:reqwest", "reqwest/rustls"]`.

### async-openai 0.41.3 — verified capabilities (source-read)

- License MIT (Cargo.toml `license = "MIT"` + GitHub license object). Current 0.41.3 (2026-07-31).
- **Tool-calling types exposed**: `ChatCompletionTool { function: FunctionObject }`,
  assistant message carries `tool_calls: Option<Vec<ChatCompletionMessageToolCalls>>`
  (enum over `Function(ChatCompletionMessageToolCall { id, function: FunctionCall })` and a
  newer `Custom` tool-call variant), tool results sent as `ChatCompletionRequestToolMessage
  { content, tool_call_id }` (types/chat/chat_.rs). Roles enum: System/User/Assistant/Tool/Function.
- **Per-request token usage exposed** — `CompletionUsage { prompt_tokens: u32,
  completion_tokens: u32, total_tokens: u32, prompt_tokens_details: Option<PromptTokensDetails>,
  completion_tokens_details: Option<CompletionTokensDetails> }`; details include
  `cached_tokens` and `reasoning_tokens` (types/shared/prompt_tokens_details.rs,
  completion_tokens_details.rs) — enough for R6 cost accounting without extra parsing.
- **Streaming usage chunks exposed**: `ChatCompletionStreamOptions { include_usage }` exists on
  the request; `CreateChatCompletionStreamResponse.usage: Option<CompletionUsage>` — "only
  present when you set `stream_options: {include_usage: true}`… null except for the last chunk"
  (types/chat/chat_.rs lines ~1181–1200). Client API: `Chat::create()` (errors if `stream:true`)
  and `Chat::create_stream()` returning parsed-SSE `ChatCompletionResponseStream` until `[DONE]`.
- **Base-URL override for compatible endpoints**: `OpenAIConfig::with_api_base(...)` /
  `OPENAI_BASE_URL` env (config.rs) — works for DeepSeek (`https://api.deepseek.com`),
  OpenRouter (`https://openrouter.ai/api/v1`), Ollama (`http://localhost:11434/v1`), vLLM.
- **Built-in retry middleware** (feature `middleware`, on tower): `SimpleRetryPolicy` retries
  429 + 5xx + connect errors, honors `Retry-After`, else exponential 100 ms × 2^n capped 8 s,
  default 3 attempts (src/middleware/retry/{mod.rs,openai.rs}).
- Feature flags are fine-grained (`chat-completion`, `middleware`, `byot`, …) — you can compile
  in only chat-completions and keep the binary lean.
- Caveat: `Function`-role messages and `function_call` are deprecated in favor of `tool_calls`;
  DeepSeek/Ollama never supported the legacy shape — always use `tools`.

### SSE crates

Two viable layers, choose one (do not stack both):
- `reqwest-eventsource` 0.6.0 (MIT OR Apache-2.0; repo jpopesculian/reqwest-eventsource;
  last release 2024-03 — stable but aging). `EventSource::new(request)` → `Stream<Item =
  Result<Event, Error>>` with auto-reconnect and a `RetryPolicy` (default exponential 300 ms
  ×2 cap 5 s — the same defaults rig uses). Good when you hand-roll the chat JSON yourself.
- `eventsource-stream` 0.2.3 (MIT OR Apache-2.0) — pure Stream adapter used by async-openai
  itself (`eventsource-stream = { version = "0.2", optional = true }` in its Cargo.toml).
  If you use async-openai you never touch SSE crates; its `create_stream` is already parsed.
- Hand-rolled alternative: `http-body-util` + `tokio-util` codec over `reqwest::Response::bytes_stream()`
  parsing `data:` lines — rig does exactly this internally (providers/internal/sse_transport.rs).

### Chat-completions tool-calling wire format (OpenAI spec, verified in openai-node source)

Request: `tools: [{ type: "function", function: { name, description, parameters: <JSON Schema> } }]`,
optional `tool_choice` (`"auto" | "none" | "required" | {type:"function",function:{name}}`).
Response (assistant turn): `message.tool_calls[] = { id, type: "function", function: { name,
arguments: <string of JSON, not an object> } }` with `finish_reason: "tool_calls"`.
Continuation: append the assistant message verbatim, then one message per call:
`{ role: "tool", tool_call_id: <id>, content: <string result> }`, re-send `tools`,
loop until `finish_reason == "stop"` (or budget/stop-condition hits). `arguments` is a JSON
*string* — model output is not guaranteed valid; validate/repair before executing
(openai-node doc comment: "the model does not always generate valid JSON, and may hallucinate
parameters"). Streaming deltas carry `tool_calls[i].{index, id?, function?{name?, arguments?}}`
fragmented by `index` — accumulate arguments per index before parsing
(async-openai `ChatCompletionMessageToolCallChunk`). Full `finish_reason` union:
`stop | length | tool_calls | content_filter | function_call` (completions.ts L428).

### Protocol differences that affect usage accounting (verified in each provider's source/docs)

| Endpoint | `include_usage` | usage fields | quirks |
|---|---|---|---|
| OpenAI `/v1/chat/completions` | send `stream_options.include_usage`; final chunk has empty `choices[]`, other chunks `usage:null`; stream interrupted → may lose final usage chunk | prompt/completion/total + `prompt_tokens_details.cached_tokens`, `completion_tokens_details.reasoning_tokens` | ground-truth wire shape others diverge from |
| DeepSeek `api.deepseek.com` | DeepSeek reports usage on the final chunk; rig's DeepSeek `Ext` keeps `STREAM_INCLUDE_USAGE=true` | prompt/completion/total **+ `prompt_cache_hit_tokens`, `prompt_cache_miss_tokens`**, `completion_tokens_details.reasoning_tokens` | thinking mode **ignores** temperature/top_p/presence/frequency_penalty silently; with `tools`, `reasoning_content` of **all previous turns must be echoed back** (context/cost bloat); strict mode via `/beta` + `strict:true`; pricing split peak/off-peak (v4-flash: hit $0.007/0.014, miss $0.22/0.44, out $0.66/1.32 per 1M) |
| OpenRouter `openrouter.ai/api/v1` | `include_usage`/`stream_options.include_usage` are **deprecated no-ops** — usage always returned | adds `usage.cost` (credits) + `cost_details.upstream_inference_cost`; `prompt_tokens_details.{cached_tokens,cache_write_tokens}`; `completion_tokens_details.reasoning_tokens` | final usage chunk **deviates**: contains one choice with a content-free delta repeating `finish_reason`+`native_finish_reason` (not empty `choices`) — treat as accounting frame, not a second turn |
| Ollama `/v1/chat/completions` | supported (checkbox ✓ in openai-compat doc) | `usage.prompt_tokens = prompt_eval_count`, `completion_tokens = eval_count`, `total = sum`; **no** cached/reasoning breakdown (verified in `openai/openai.go` `ToUsage`) | `tool_choice`, `n`, `logit_bias`, `user` **not supported**; `num_ctx` not API-settable (Modelfile `PARAMETER num_ctx` + `ollama create`); older daemons emit id-less tool calls (rig comment) |
| vLLM `/v1/chat/completions` | supported; also `continuous_usage_stats` | `UsageInfo` + `prompt_tokens_details.{cached_tokens, created_cache_tokens, multimodal_tokens}`, `completion_tokens_details.reasoning_tokens` | `user` ignored; requires a `chat_template`; `parallel_tool_calls:false` = max-one tool call per request |

### Cost accounting design (config R3 + R6)

Config table `[[model.prices]]` per 1M tokens: `prompt_miss`, `prompt_hit`, `completion`,
`reasoning` ( billed at completion rate when listed). Per-call cost:
`(prompt_tokens - cached)·p_miss + cached·p_hit + completion_tokens·p_completion`
(reasoning tokens are already inside `completion_tokens` — do not double-count).
Provider shortcuts: DeepSeek gives hit/miss directly (`prompt_cache_hit_tokens`,
`prompt_cache_miss_tokens`); OpenRouter gives `usage.cost` already in credits. Provider-reported
usage is **ground truth** (DeepSeek docs: "the actual number of tokens processed each time is
based on the model's return") — never trust local tokenizers for billing. Persist per-call usage
in the ledger; running cost = Σ; R6 interrupt when running ≥ `config.budget` (also a
model-call-count budget, see budget section).

### Token counting (preflight estimates only)

tiktoken-rs 0.12.0 (MIT, repo active 2026-07) encodings: `o200k_harmony` (gpt-oss-20b/120b),
`o200k_base` (GPT-5/o3/o4/gpt-4o/gpt-4.1/codex), `cl100k_base` (gpt-4/3.5-turbo). **Only OpenAI
models** are accurate; DeepSeek/ollama use their own BPE (DeepSeek ships a `deepseek_tokenizer.zip`
demo) — local estimates for those are approximate, label them as such. Use tiktoken only to
pre-check "will this fit + how much budget remains before sending", never to bill.

### Context-window management: truncate, don't LLM-summarize

Two textbook strategies — (a) truncate oldest turns, (b) LLM-summarize older history. For Disk
Janitor use **(a) with a deterministic digest**: tool results are already aggregate stats, so
replace any tool result older than N turns with a one-line ledger receipt
(`"C-007: applied 2.1 GB, grade RebuildableFromSource"`) — zero new tokens, no privacy risk,
reproducible. Never route history through a second model call for summarization (extra cost +
extra privacy exposure). Hard cap at `config.ctx_len` from R3 × a safety factor; pre-check with
tiktoken before sending so you stop *before* a `context_length_exceeded` 400.

### Budget enforcement & stop conditions

Three independent stop counters, checked every turn:
1. **Model-call budget** (rig `PromptRequest::max_turns(n)` pattern → `PromptError::MaxTurnsError`;
   default 1, set ≥3 for tool loops). 2. **Token budget** — Σ `usage.total_tokens` ≥ budget.
3. **Cost budget** — Σ cost ≥ `config.budget` (R6). Plus natural stops: `finish_reason == stop`
(not `tool_calls`); explicit `Stop` tool (swiftide pattern: model calls a `stop` control tool →
`StopReason::RequestedByTool`); tool-failure loop cap (swiftide `tool_retry_limit=3`, hashed per
call name+args so a stuck tool can't spin forever); user Ctrl-C (WAL-consistent per SPEC §3.2).
Recommended stop tool name for Disk Janitor: `record_verdict` — the model's terminal action.

### How rig and swiftide structure the agent loop (borrowable patterns)

| Crate | version / license | loop shape | tool registry | stop conditions |
|---|---|---|---|---|
| rig-core + rig-agent + rig-run | 0.42.0 / MIT | `AgentRun` **sans-IO steppable serializable state machine**: `next_step() → AgentRunStep::{CallModel{prompt,history,turn}, CallTools{calls}, Done(response)}`; `max_turns` model-call budget → `MaxTurnsError`; `extended_details()` returns `PromptResponse{ output, usage(aggregate), completion_calls: Vec<CompletionCall> }` | `Tool` trait (`const NAME`, typed `Args`/`Output`), `ToolDefinition{name,description,parameters:JSONSchema}`, `ToolSet`/`ToolCatalog`; capability flags per provider (`SUPPORTS_TOOLS`, `STREAM_INCLUDE_USAGE`, `EMITS_COMPLETE_SINGLE_CHUNK_TOOL_CALLS`) | `MaxTurnsError`; invalid-tool-call hooks can `repair`/`skip`/`fail`; `on_completion_response`/`on_model_turn_finished` hooks can accept/retry/stop |
| swiftide-agents | 0.32.1 / MIT (bosun-ai) | `Agent{hooks, context, tools: HashSet<Box<dyn Tool>>, llm: Box<dyn ChatCompletion>, limit: Option<usize>, tool_retry_limit=3}`; `run()` loops calling `step()` until stopped | `Tool`/`ToolBox` traits + `Toolset`; built-in control tools `Stop`, `StopWithArgs`, `AgentCanFail`, `ApprovalRequired` | `StopReason` enum: `RequestedByTool(tool_call,msg)`, `AgentFailed`, `ToolCallsOverLimit`, `FeedbackRequired{tool_call,payload}`, `Error`, `NoNewMessages`, `Other` |

Borrow from **rig-run** the sans-IO state machine (testable without network; resumable), and from
**swiftide** the `StopReason` enum + control-tool pattern (the model declares completion via a tool,
not by stopping text). Both are MIT — safe to vendor/copy.

### Privacy: redaction / aggregation layer (hard constraint)

SPEC §3.3 invariant: **the model never sees file paths or filenames — only the intent sentence +
aggregate statistics.** Enforce at the *tool-result serializer*, the single chokepoint, not by
trusting prompt construction:
1. `scan`/`plan` tools return candidates as opaque IDs (`C-001…`, deterministic size-desc order);
   per-candidate payload = `{ id, bytes, grade, evidence_summary[], mtime_bucket,
   dir_shape: "node_modules-like tree" }` — **no** absolute paths, usernames, or `~`.
2. `execute` tools accept **only ID lists**; the ID→path map lives in the ledger, resolved inside
   `execute/` after the agent decision, never crossing the serialization boundary back to the model.
3. Tool-result receipts echoed to the model are redacted too (only `{id, verdict, bytes, grade}`).
4. The intent sentence from the NL policy compiler is user-authored — pass through verbatim.
5. CI assertion test: serde_json of every outbound message must reject regexes for `/`, `~`,
   `\.(app|trash|dmg)$`, `/Users/`, `/Volumes/` — fail the build if any leak (mirrors SPEC's
   "iOS dataless files don't get evicted" discipline: scan never passes paths outward).

## Reusable code & licenses

| Repo | License | What to borrow | Where (file/module) |
|---|---|---|---|
| 0xPlaygrounds/rig | MIT | sans-IO `AgentRun` state machine (`next_step`/`CallModel`/`CallTools`/`Done`, `max_turns`→`MaxTurnsError`, usage aggregation); `Tool`/`ToolDefinition`/`ToolCatalog` types | crates/rig-run/src/run.rs; crates/rig-core/src/tool/mod.rs, completion/request.rs |
| bosun-ai/swiftide | MIT | `StopReason` enum + control tools (`Stop`/`AgentCanFail`/`ApprovalRequired`); `tool_retry_limit` hashed-per-call pattern; agent-hook lifecycle | swiftide-agents/src/{state.rs, tools/control.rs, hooks.rs} |
| 64bit/async-openai | MIT | typed `ChatCompletionRequest*`/`CompletionUsage`/`ChatCompletionStreamOptions`; `OpenAIConfig::with_api_base`; `SimpleRetryPolicy` (429/5xx + Retry-After) | async-openai/src/types/chat/chat_.rs; config.rs; src/middleware/retry/ |
| ollama/ollama | MIT | reference for mapping native metrics→OpenAI usage (`ToUsage`: `prompt_eval_count`→`prompt_tokens`); id-less tool-call handling | openai/openai.go |
| vllm-project/vllm | Apache-2.0 | `UsageInfo`/`StreamOptions`/token-detail structs to mirror in our cost model | vllm/entrypoints/openai/engine/protocol.py |
| openai/openai-node | Apache-2.0 | the canonical chat-completions TS types (finish_reason union, include_usage doc comments) | src/resources/chat/completions/completions.ts |

## Recommended crates

| Crate | Purpose | Why / notes | Alternative |
|---|---|---|---|
| async-openai 0.41.3 | typed client + SSE + retry for chat-completions | MIT; default rustls; `with_api_base` covers all 4 endpoints; exposes `CompletionUsage` + streaming usage chunk; built-in `SimpleRetryPolicy` | hand-roll with reqwest+serde_json |
| reqwest 0.13.4 | HTTP, rustls default | MIT OR Apache-2.0; rustls+aws-lc+platform-verifier default since 0.13; built-in `ClientBuilder::retry` | hyper + hyper-rustls |
| eventsource-stream 0.2.3 | SSE parsing if hand-rolling | MIT OR Apache-2.0; what async-openai uses internally; stable since 2022 | reqwest-eventsource |
| reqwest-eventsource 0.6.0 | SSE with auto-reconnect if not using async-openai | MIT OR Apache-2.0; aging (2024-03) | eventsource-stream + manual reconnect |
| tiktoken-rs 0.12.0 | OpenAI-model token preflight estimates | MIT; `o200k_base`/`o200k_harmony`/`cl100k_base`; accurate only for OpenAI models | approximate char/4 heuristic |
| backon 1.6.0 | retry wrapper for tool calls / connect errors | Apache-2.0; `ExponentialBuilder`/`ConstantBuilder`/`FibonacciBuilder`; `.when()`/`.notify()`; Retry-After support | tower::retry |
| tower 0.5.3 | retry/budget + layer primitives | MIT; `tower::retry::Policy`/`Budget` (async-openai's middleware is built on it) | backon |
| serde_json | wire-format (de)serialization | already a reqwest/async-openai dep; tool `arguments` is a JSON string | — |
| tokio 1.53.1 | async runtime | only need `rt-multi-thread` + `macros` + `sync` | smol |

## Risks & open questions

- **DeepSeek thinking-mode context bloat**: with `tools`, `reasoning_content` of *all* previous
  turns must be echoed back — reasoning tokens (billed at completion rate) accumulate fast in a
  multi-tool loop. Mitigation: cap DeepSeek to non-thinking mode for agent loops, or set
  `reasoning_effort: low` and track `completion_tokens_details.reasoning_tokens` against budget.
- **OpenRouter usage-chunk shape deviation**: final chunk carries a content-free delta, not empty
  `choices[]` — naive `choices[0].delta.content` access must guard for the accounting frame, or
  cost gets mis-attributed. Parse `usage` on that chunk, ignore its `choices`.
- **Lost final usage chunk**: OpenAI/Ollama/vLLM streaming can drop the final usage chunk if the
  stream is interrupted (client cancel, network). R6 cost accounting must fall back to tiktoken
  *estimate* (clearly marked "unverified") when `usage` is absent — but never bill on estimates.
  Prefer non-streaming for the agent loop itself; stream only the TUI text display.
- **`arguments` validity**: tool `arguments` is a JSON string the model may malform; the loop must
  repair/reject (rig hooks: `repair`/`skip`/`fail`; swiftide: `tool_retry_limit`). Without this a
  single bad call aborts the whole run.
- **Ollama id-less tool calls**: older daemons omit `tool_call.id` → `ChatCompletionRequestToolMessage`
  needs a synthetic id. rig mints one (`ToolCallId::for_provider`); mirror that.
- **Streaming `include_usage` on Ollama**: doc shows checkbox ✓ but older Ollama versions may not
  emit it — gate on a config flag + version probe, fall back to `stream:false`.
- **Privacy leak vector via tool descriptions**: tool `description`/`name` and JSON-Schema
  `description` fields are visible to the model and to provider logs — must not contain
  path/name hints (e.g. "delete a node_modules dir" leaks a name; use "apply a reclaim verdict").
- **macOS non-goal**: no syscall claims in this lane; all API claims verified against upstream
  source/docs, not memory.
- **Open question**: should the agent loop be sans-IO (rig-run style, testable/resumable) or
  inline (swiftide style, simpler)? Recommend sans-IO — matches SPEC's WAL/crash-recovery ethos.

## Sources

1. crates.io API — async-openai 0.41.3 (MIT), reqwest 0.13.4 (MIT OR Apache-2.0), reqwest-eventsource 0.6.0 (MIT OR Apache-2.0), eventsource-stream 0.2.3 (MIT OR Apache-2.0), tiktoken-rs 0.12.0 (MIT), backon 1.6.0 (Apache-2.0), tower 0.5.3 (MIT), rig-core 0.42.0 (MIT), swiftide 0.32.1 (MIT), tokio 1.53.1 (MIT OR Apache-2.0) — https://crates.io/api/v1/crates/<name> — accessed 2026-08-30
2. async-openai source (chat types, stream options, config, retry middleware) — https://github.com/64bit/async-openai/blob/main/async-openai/src/types/chat/chat_.rs, config.rs, src/middleware/retry/{mod.rs,openai.rs}, Cargo.toml — accessed 2026-08-30
3. reqwest CHANGELOG (0.13.0 rustls-default break, 0.13.4 release) — https://github.com/seanmonstar/reqwest/blob/master/CHANGELOG.md — accessed 2026-08-30
4. openai-node chat completions source (finish_reason union, include_usage doc comment, Function type) — https://github.com/openai/openai-node/blob/main/src/resources/chat/completions/completions.ts — accessed 2026-08-30
5. OpenAI API reference: Create chat completion (stream_options.include_usage semantics) — https://developers.openai.com/api/reference/resources/completions/methods/create/ — accessed 2026-08-30
6. Ollama OpenAI compatibility doc (supported features/fields checkboxes) — https://docs.ollama.com/api/openai-compatibility — accessed 2026-08-30
7. Ollama native /api/chat doc (prompt_eval_count/eval_count) + openai.go ToUsage — https://github.com/ollama/ollama/blob/main/docs/api.md, openai/openai.go — accessed 2026-08-30
8. DeepSeek API docs: tool calls, thinking mode (reasoning_content echo rule), pricing, token usage — https://api-docs.deepseek.com/guides/{tool_calls,thinking_mode,quick_start/pricing,quick_start/token_usage} — accessed 2026-08-30
9. OpenRouter usage accounting + streaming docs (cost/cost_details, deprecated include_usage, usage-chunk deviation) — https://openrouter.ai/docs/use-cases/usage-accounting, /docs/api-reference/streaming — accessed 2026-08-30
10. vLLM OpenAI-compatible server doc + protocol.py (UsageInfo, StreamOptions, token details) — https://github.com/vllm-project/vllm/blob/main/docs/serving/online_serving/openai_compatible_server.md, vllm/entrypoints/openai/engine/protocol.py — accessed 2026-08-30
11. rig source: AgentRun state machine, PromptRequest/Usage, Tool trait/ToolDefinition, provider Ext flags, deepseek/ollama/openai providers — https://github.com/0xPlaygrounds/rig/blob/main/crates/{rig-run/src/run.rs, rig-agent/src/agent/prompt_request/mod.rs, rig-core/src/tool/mod.rs, completion/request.rs, providers/{deepseek.rs,ollama.rs,openai/completion/mod.rs,streaming.rs}} — accessed 2026-08-30
12. swiftide source: Agent struct, StopReason enum, control tools, hooks — https://github.com/bosun-ai/swiftide/blob/main/swiftide-agents/src/{agent.rs,state.rs,tools/control.rs,hooks.rs} — accessed 2026-08-30
13. tiktoken-rs README (encoding→model table, features) — https://github.com/zurawiki/tiktoken-rs/blob/main/README.md — accessed 2026-08-30
14. backon README (Retryable, ExponentialBuilder) — https://github.com/Xuanwo/backon/blob/main/README.md — accessed 2026-08-30
