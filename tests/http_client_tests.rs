//! HTTP ChatClient integration tests (feature `llm`) against a local TCP stub.
//! No real API calls: a hand-rolled HTTP/1.1 server on `127.0.0.1:0` serves a
//! scripted sequence of (status, body) responses and records every request.

#![cfg(feature = "llm")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use crossbeam_channel::{Receiver, unbounded};
use disk_janitor::agent::http::HttpChatClient;
use disk_janitor::agent::{ChatClient, Msg, Role, ToolCall, ToolDef, Usage};
use disk_janitor::config::Config;
use serde_json::json;

/// Every request the stub served: request head (request line + headers) and parsed body.
struct Stub {
    url: String,
    requests: Receiver<(String, serde_json::Value)>,
    served: Arc<AtomicUsize>,
}

impl Stub {
    /// Drain all requests received so far (non-blocking).
    fn requests(&self) -> Vec<(String, serde_json::Value)> {
        self.requests.try_iter().collect()
    }
    fn served(&self) -> usize {
        self.served.load(Ordering::SeqCst)
    }
}

/// Serve `responses` in order; once exhausted, answer 500 with an error body.
fn spawn_stub(responses: Vec<(u16, &'static str)>) -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub server");
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = unbounded::<(String, serde_json::Value)>();
    let served = Arc::new(AtomicUsize::new(0));
    let served_c = served.clone();
    thread::spawn(move || {
        let mut responses = responses.into_iter();
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            served_c.fetch_add(1, Ordering::SeqCst);
            let (head, body) = read_request(&mut stream);
            let body_json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
            tx.send((head, body_json)).expect("send request to test");
            let (status, payload) = match responses.next() {
                Some(r) => r,
                None => (500, r#"{"error":{"message":"stub script exhausted"}}"#),
            };
            write_response(&mut stream, status, payload);
        }
    });
    Stub {
        url,
        requests: rx,
        served,
    }
}

fn read_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut head = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).expect("read head line") == 0 {
            break;
        }
        if line == "\r\n" {
            break;
        }
        head.push_str(&line);
    }
    let len: usize = head
        .lines()
        .map(|l| l.to_ascii_lowercase())
        .find_map(|l| {
            l.strip_prefix("content-length:")
                .and_then(|v| v.trim().parse().ok())
        })
        .unwrap_or(0);
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).expect("read body");
    (head, body)
}

fn write_response(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(resp.as_bytes()).expect("write response");
    stream.flush().expect("flush response");
}

fn cfg(endpoint: &str) -> Config {
    Config {
        endpoint: endpoint.to_string(),
        api_key: Some("test-key".into()),
        model: "test-model".into(),
        ..Config::default()
    }
}

fn msgs() -> Vec<Msg> {
    vec![
        Msg {
            role: Role::System,
            content: "you are a reclaim agent".into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        },
        Msg {
            role: Role::User,
            content: "腾出 25 GB".into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        },
        Msg {
            role: Role::Assistant,
            content: String::new(),
            tool_calls: vec![ToolCall {
                id: "call_echo".into(),
                name: "scan_summary".into(),
                arguments: json!({}),
            }],
            tool_call_id: None,
        },
        Msg {
            role: Role::Tool,
            content: r#"{"t":"scan_summary","volumes":1,"files":10,"bytes":100,"candidates":2,"recoverable_bytes":80}"#.into(),
            tool_calls: Vec::new(),
            tool_call_id: Some("call_echo".into()),
        },
    ]
}

fn tool_defs() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "scan_summary".into(),
        description:
            "Report volume and candidate totals from the current scan. No item-level detail.".into(),
        parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
    }]
}

const HAPPY: &str = r#"{
  "id": "chatcmpl-test",
  "object": "chat.completion",
  "created": 0,
  "model": "test-model",
  "choices": [{
    "index": 0,
    "message": {
      "role": "assistant",
      "content": null,
      "tool_calls": [
        {"type": "function", "id": "call_abc", "function": {"name": "scan_summary", "arguments": "{}"}},
        {"type": "function", "id": "call_def", "function": {"name": "candidates", "arguments": "{\"filter\":{\"grade\":\"pure_cache\",\"limit\":3}}"}}
      ]
    },
    "finish_reason": "tool_calls"
  }],
  "usage": {"prompt_tokens": 120, "completion_tokens": 45, "total_tokens": 165, "prompt_tokens_details": {"cached_tokens": 20}}
}"#;

const STOP: &str = r#"{
  "id": "chatcmpl-stop",
  "object": "chat.completion",
  "created": 0,
  "model": "test-model",
  "choices": [{
    "index": 0,
    "message": {"role": "assistant", "content": "plan ready: build_plan, then apply_plan"},
    "finish_reason": "stop"
  }],
  "usage": {"prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10}
}"#;

// --- tests ------------------------------------------------------------------------

/// (a) Happy tool-call response → ModelReply with parsed argument Values, usage with
/// cached tokens; the outgoing request carries the mapped trajectory + tool defs.
#[test]
fn tool_call_response_parses() {
    let stub = spawn_stub(vec![(200, HAPPY)]);
    let client = HttpChatClient::new(cfg(&stub.url)).expect("client");
    let reply = client.complete(&msgs(), &tool_defs()).expect("complete");
    assert_eq!(reply.content, None);
    assert_eq!(reply.tool_calls.len(), 2);
    assert_eq!(reply.tool_calls[0].id, "call_abc");
    assert_eq!(reply.tool_calls[0].name, "scan_summary");
    assert_eq!(reply.tool_calls[0].arguments, json!({}));
    assert_eq!(reply.tool_calls[1].id, "call_def");
    assert_eq!(reply.tool_calls[1].name, "candidates");
    assert_eq!(
        reply.tool_calls[1].arguments,
        json!({"filter": {"grade": "pure_cache", "limit": 3}})
    );
    assert_eq!(
        reply.usage,
        Usage {
            prompt_tokens: 120,
            completion_tokens: 45,
            cached_tokens: 20,
        }
    );

    // Outgoing request shape.
    let requests = stub.requests();
    assert_eq!(requests.len(), 1);
    let (head, body) = &requests[0];
    assert!(head.starts_with("POST /chat/completions HTTP/1.1"));
    assert!(
        head.to_lowercase()
            .contains("authorization: bearer test-key")
    );
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["stream"], json!(false));
    assert_eq!(body["messages"].as_array().unwrap().len(), 4);
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], "you are a reclaim agent");
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][1]["content"], "腾出 25 GB");
    // Assistant echo: verbatim tool_calls with id + function{name, arguments}.
    assert_eq!(body["messages"][2]["role"], "assistant");
    assert_eq!(body["messages"][2]["tool_calls"][0]["id"], "call_echo");
    assert_eq!(
        body["messages"][2]["tool_calls"][0]["function"]["name"],
        "scan_summary"
    );
    assert_eq!(
        body["messages"][2]["tool_calls"][0]["function"]["arguments"],
        "{}"
    );
    // Tool result tagged with its call id.
    assert_eq!(body["messages"][3]["role"], "tool");
    assert_eq!(body["messages"][3]["tool_call_id"], "call_echo");
    // Tool defs as JSON-schema function defs.
    let tools = body["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["type"], "function");
    assert_eq!(tools[0]["function"]["name"], "scan_summary");
    assert_eq!(tools[0]["function"]["parameters"]["type"], "object");
}

/// Natural stop: content maps through, tool_calls empty; usage without
/// prompt_tokens_details → cached_tokens 0.
#[test]
fn stop_response_maps_content_and_usage() {
    let stub = spawn_stub(vec![(200, STOP)]);
    let client = HttpChatClient::new(cfg(&stub.url)).expect("client");
    let reply = client.complete(&msgs(), &tool_defs()).expect("complete");
    assert_eq!(
        reply.content.as_deref(),
        Some("plan ready: build_plan, then apply_plan")
    );
    assert!(reply.tool_calls.is_empty());
    assert_eq!(
        reply.usage,
        Usage {
            prompt_tokens: 7,
            completion_tokens: 3,
            cached_tokens: 0,
        }
    );
}

/// Missing usage chunk → zeroed usage (never billed on estimates, note 07 §cost).
#[test]
fn missing_usage_maps_to_zeros() {
    let body = r#"{
      "id": "chatcmpl-nousage",
      "object": "chat.completion",
      "created": 0,
      "model": "test-model",
      "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop"}]
    }"#;
    let stub = spawn_stub(vec![(200, body)]);
    let client = HttpChatClient::new(cfg(&stub.url)).expect("client");
    let reply = client.complete(&msgs(), &tool_defs()).expect("complete");
    assert_eq!(reply.usage, Usage::default());
}

/// Malformed tool arguments are preserved raw as `{"_raw": …}` (the loop validates;
/// ToolFailLoop accounting still sees the exact attempt).
#[test]
fn malformed_arguments_preserved_raw() {
    let body = r#"{
      "id": "chatcmpl-badargs",
      "object": "chat.completion",
      "created": 0,
      "model": "test-model",
      "choices": [{"index": 0, "message": {"role": "assistant", "content": null,
        "tool_calls": [{"type": "function", "id": "call_1", "function": {"name": "candidates", "arguments": "not-json"}}]},
        "finish_reason": "tool_calls"}],
      "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    }"#;
    let stub = spawn_stub(vec![(200, body)]);
    let client = HttpChatClient::new(cfg(&stub.url)).expect("client");
    let reply = client.complete(&msgs(), &tool_defs()).expect("complete");
    assert_eq!(reply.tool_calls.len(), 1);
    assert_eq!(reply.tool_calls[0].arguments, json!({"_raw": "not-json"}));
}

/// (b) Ollama id-less tool call → synthetic id `call_<idx>` (typed deserialization
/// fails; the lenient re-parse mints the id).
#[test]
fn idless_ollama_call_gets_synthetic_id() {
    let body = r#"{
      "id": "chatcmpl-ollama",
      "object": "chat.completion",
      "created": 0,
      "model": "test-model",
      "choices": [{"index": 0, "message": {"role": "assistant", "content": null,
        "tool_calls": [
          {"type": "function", "function": {"name": "scan_summary", "arguments": "{}"}},
          {"type": "function", "function": {"name": "candidates", "arguments": "{\"filter\":{}}"}}
        ]},
        "finish_reason": "tool_calls"}],
      "usage": {"prompt_tokens": 30, "completion_tokens": 9, "total_tokens": 39}
    }"#;
    let stub = spawn_stub(vec![(200, body)]);
    let client = HttpChatClient::new(cfg(&stub.url)).expect("client");
    let reply = client.complete(&msgs(), &tool_defs()).expect("complete");
    assert_eq!(reply.tool_calls.len(), 2);
    assert_eq!(reply.tool_calls[0].id, "call_0");
    assert_eq!(reply.tool_calls[0].name, "scan_summary");
    assert_eq!(reply.tool_calls[0].arguments, json!({}));
    assert_eq!(reply.tool_calls[1].id, "call_1");
    assert_eq!(reply.tool_calls[1].name, "candidates");
    assert_eq!(reply.tool_calls[1].arguments, json!({"filter": {}}));
    assert_eq!(
        reply.usage,
        Usage {
            prompt_tokens: 30,
            completion_tokens: 9,
            cached_tokens: 0,
        }
    );
}

/// (c) 500 then success → retried once by async-openai's built-in OpenAIRetryLayer
/// (429/5xx/connect, Retry-After honored), reply from attempt 2.
#[test]
fn retries_on_500_then_succeeds() {
    let err = r#"{"error":{"message":"boom","type":"server_error","param":null,"code":null}}"#;
    let stub = spawn_stub(vec![(500, err), (200, HAPPY)]);
    let client = HttpChatClient::new(cfg(&stub.url)).expect("client");
    let reply = client.complete(&msgs(), &tool_defs()).expect("complete");
    assert_eq!(reply.tool_calls.len(), 2);
    assert_eq!(
        stub.served(),
        2,
        "expected exactly 2 requests (1 failed + 1 retry)"
    );
}

/// 4xx (outside the built-in layer's 429/5xx/connect set) surfaces as an error
/// without further attempts.
#[test]
fn four_xx_fails_fast_without_retry() {
    let err = r#"{"error":{"message":"invalid api key","type":"invalid_request_error","param":null,"code":"invalid_api_key"}}"#;
    let stub = spawn_stub(vec![(401, err)]);
    let client = HttpChatClient::new(cfg(&stub.url)).expect("client");
    let err = client
        .complete(&msgs(), &tool_defs())
        .expect_err("must fail");
    assert!(err.to_string().contains("invalid api key"), "got: {err}");
    assert_eq!(stub.served(), 1, "4xx must not retry");
}

/// (d) Missing api_key → constructor error naming the env var / config.
#[test]
fn missing_api_key_is_an_error() {
    let c = Config {
        api_key: None,
        ..cfg("http://127.0.0.1:9") // port 9 (discard) — must never be contacted
    };
    let err = match HttpChatClient::new(c) {
        Ok(_) => panic!("constructor must fail without an api key"),
        Err(e) => e,
    };
    assert!(
        err.to_string().contains("DISK_JANITOR_API_KEY"),
        "got: {err}"
    );
}
