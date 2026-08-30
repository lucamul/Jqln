//! Talking to Anthropic and OpenAI. Both are called with `stream: true` and
//! read as Server-Sent Events on a worker thread; tokens and the final usage
//! come back over a channel.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

/// A message in the conversation sent to the model.
#[derive(Clone)]
pub struct Msg {
    pub role: &'static str, // "user" | "assistant"
    pub text: String,
}

/// What the worker thread streams back.
pub enum Event {
    Token(String),
    Done { input: u64, output: u64 },
    Error(String),
}

/// Everything a provider needs for one turn.
pub struct Request {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub system: String,
    pub messages: Vec<Msg>,
    /// Overridable for tests / self-hosted gateways.
    pub base_url: Option<String>,
}

/// Run the request on the current thread, emitting [`Event`]s until done,
/// cancelled, or errored.
pub fn run(req: Request, tx: &Sender<Event>, cancel: &Arc<AtomicBool>) {
    let result = match req.provider.as_str() {
        "openai" => openai(&req, tx, cancel),
        "anthropic" => anthropic(&req, tx, cancel),
        other => Err(format!("unknown provider {other:?} (use \"anthropic\" or \"openai\")")),
    };
    match result {
        Ok(()) => {}
        Err(e) => {
            let _ = tx.send(Event::Error(e));
        }
    }
}

fn anthropic(req: &Request, tx: &Sender<Event>, cancel: &Arc<AtomicBool>) -> Result<(), String> {
    let url = format!(
        "{}/v1/messages",
        req.base_url.as_deref().unwrap_or("https://api.anthropic.com")
    );
    let body = json!({
        "model": req.model,
        "max_tokens": 4096,
        "system": req.system,
        "stream": true,
        "messages": req.messages.iter()
            .map(|m| json!({ "role": m.role, "content": m.text }))
            .collect::<Vec<_>>(),
    });

    let resp = ureq::post(&url)
        .set("x-api-key", &req.api_key)
        .set("anthropic-version", "2023-06-01")
        .set("content-type", "application/json")
        .send_string(&body.to_string());

    let reader = open_stream(resp)?;
    let (mut input, mut output) = (0u64, 0u64);

    for line in reader.lines() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let line = line.map_err(|e| e.to_string())?;
        let Some(data) = line.strip_prefix("data: ") else { continue };
        let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
        match v.get("type").and_then(Value::as_str) {
            Some("content_block_delta") => {
                if let Some(t) = v.pointer("/delta/text").and_then(Value::as_str)
                    && !t.is_empty()
                {
                    let _ = tx.send(Event::Token(t.to_string()));
                }
            }
            Some("message_start") => {
                input = v
                    .pointer("/message/usage/input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
            }
            Some("message_delta") => {
                if let Some(o) = v.pointer("/usage/output_tokens").and_then(Value::as_u64) {
                    output = o;
                }
            }
            Some("message_stop") => break,
            Some("error") => {
                let msg = v
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("stream error");
                return Err(msg.to_string());
            }
            _ => {}
        }
    }
    let _ = tx.send(Event::Done { input, output });
    Ok(())
}

fn openai(req: &Request, tx: &Sender<Event>, cancel: &Arc<AtomicBool>) -> Result<(), String> {
    let url = format!(
        "{}/v1/chat/completions",
        req.base_url.as_deref().unwrap_or("https://api.openai.com")
    );
    let mut messages = vec![json!({ "role": "system", "content": req.system })];
    messages.extend(
        req.messages
            .iter()
            .map(|m| json!({ "role": m.role, "content": m.text })),
    );
    let body = json!({
        "model": req.model,
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": messages,
    });

    let resp = ureq::post(&url)
        .set("authorization", &format!("Bearer {}", req.api_key))
        .set("content-type", "application/json")
        .send_string(&body.to_string());

    let reader = open_stream(resp)?;
    let (mut input, mut output) = (0u64, 0u64);

    for line in reader.lines() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let line = line.map_err(|e| e.to_string())?;
        let Some(data) = line.strip_prefix("data: ") else { continue };
        if data.trim() == "[DONE]" {
            break;
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
        if let Some(t) = v.pointer("/choices/0/delta/content").and_then(Value::as_str)
            && !t.is_empty()
        {
            let _ = tx.send(Event::Token(t.to_string()));
        }
        if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
            input = u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(input);
            output = u.get("completion_tokens").and_then(Value::as_u64).unwrap_or(output);
        }
    }
    let _ = tx.send(Event::Done { input, output });
    Ok(())
}

/// Turn ureq's result into a line reader, or a readable error.
fn open_stream(
    resp: Result<ureq::Response, ureq::Error>,
) -> Result<BufReader<Box<dyn std::io::Read + Send + Sync + 'static>>, String> {
    match resp {
        Ok(r) => Ok(BufReader::new(r.into_reader())),
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            let detail = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|v| {
                    v.pointer("/error/message")
                        .or_else(|| v.get("message"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_else(|| body.chars().take(200).collect());
            Err(format!("HTTP {code}: {detail}"))
        }
        Err(e) => Err(e.to_string()),
    }
}
