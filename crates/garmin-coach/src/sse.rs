//! Reads an OpenAI-style SSE stream.
//!
//! Text deltas go out as they arrive. Tool calls come in fragments: the first
//! chunk has the id and the name, and later chunks append to `arguments`. They
//! are addressed by `index`, not by id.

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::turn::{Event, EventSink};

#[derive(Default, Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Default, Debug)]
pub struct StreamResult {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Default)]
pub struct SseParser {
    buf: String,
    out: StreamResult,
    by_index: Vec<ToolCall>,
    events: usize,
}

impl SseParser {
    /// Take the next bytes. A partial event waits in the buffer.
    pub fn feed(&mut self, bytes: &[u8], sink: &dyn EventSink) -> Result<()> {
        self.buf.push_str(&String::from_utf8_lossy(bytes));
        while let Some((end, skip)) = event_end(&self.buf) {
            let raw: String = self.buf.drain(..end + skip).collect();
            for line in raw[..end].lines() {
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                self.events += 1;
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                if let Ok(v) = serde_json::from_str::<Value>(data) {
                    self.chunk(&v, sink)?;
                }
            }
        }
        Ok(())
    }

    /// How many `data:` lines came so far. A comment line does not count.
    pub fn data_events(&self) -> usize {
        self.events
    }

    fn chunk(&mut self, v: &Value, sink: &dyn EventSink) -> Result<()> {
        // OpenRouter reports an upstream failure inside a 200 stream.
        if let Some(err) = v.get("error") {
            let msg = err["message"]
                .as_str()
                .unwrap_or("the model returned an error");
            return Err(anyhow!(msg.to_string()));
        }
        let delta = &v["choices"][0]["delta"];
        if let Some(text) = delta["content"].as_str().filter(|t| !t.is_empty()) {
            self.out.content.push_str(text);
            sink.emit(Event::Delta { text: text.into() });
        }
        for c in delta["tool_calls"].as_array().into_iter().flatten() {
            self.fragment(c);
        }
        Ok(())
    }

    fn fragment(&mut self, c: &Value) {
        let idx = c["index"].as_u64().unwrap_or(0) as usize;
        if self.by_index.len() <= idx {
            self.by_index.resize_with(idx + 1, ToolCall::default);
        }
        let slot = &mut self.by_index[idx];
        if let Some(id) = c["id"].as_str().filter(|s| !s.is_empty()) {
            slot.id = id.into();
        }
        if let Some(name) = c["function"]["name"].as_str().filter(|s| !s.is_empty()) {
            slot.name = name.into();
        }
        if let Some(args) = c["function"]["arguments"].as_str() {
            slot.arguments.push_str(args);
        }
    }

    pub fn finish(mut self) -> StreamResult {
        self.out.tool_calls = self
            .by_index
            .into_iter()
            .filter(|c| !c.name.is_empty())
            .map(|mut c| {
                // Some providers leave the id out for a single call.
                if c.id.is_empty() {
                    c.id = format!("call_{}", c.name);
                }
                if c.arguments.trim().is_empty() {
                    c.arguments = "{}".into();
                }
                c
            })
            .collect();
        self.out
    }
}

/// The end of the first complete event, and the length of its separator.
fn event_end(buf: &str) -> Option<(usize, usize)> {
    let lf = buf.find("\n\n").map(|i| (i, 2));
    let crlf = buf.find("\r\n\r\n").map(|i| (i, 4));
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Rec(Mutex<Vec<Event>>);
    impl EventSink for Rec {
        fn emit(&self, e: Event) {
            self.0.lock().unwrap().push(e);
        }
    }

    #[test]
    fn text_and_split_tool_calls_are_reassembled() {
        let rec = Rec::default();
        let mut p = SseParser::default();
        let a = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
                 data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"sleep\",\"arguments\":\"{\\\"da\"}}]}}]}\n\n";
        // The second event is cut in the middle, then finished in a new chunk.
        let (head, tail) = a.split_at(a.len() - 10);
        p.feed(head.as_bytes(), &rec).unwrap();
        p.feed(tail.as_bytes(), &rec).unwrap();
        let b = "data: {\"choices\":[{\"delta\":{\"content\":\"lo\",\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"ys\\\":7}\"}}]}}]}\r\n\r\ndata: [DONE]\n\n";
        p.feed(b.as_bytes(), &rec).unwrap();
        let out = p.finish();
        assert_eq!(out.content, "Hello");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].id, "c1");
        assert_eq!(out.tool_calls[0].arguments, "{\"days\":7}");
        assert_eq!(rec.0.lock().unwrap().len(), 2);
    }

    #[test]
    fn an_error_inside_the_stream_fails() {
        let mut p = SseParser::default();
        let err = p
            .feed(
                b"data: {\"error\":{\"message\":\"boom\"}}\n\n",
                &Rec::default(),
            )
            .unwrap_err();
        assert_eq!(err.to_string(), "boom");
    }

    #[test]
    fn a_call_without_id_or_arguments_is_filled_in() {
        let mut p = SseParser::default();
        let chunk = b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"coach\"}}]}}]}\n\n";
        p.feed(chunk, &Rec::default()).unwrap();
        let call = &p.finish().tool_calls[0];
        assert_eq!(call.id, "call_coach");
        assert_eq!(call.arguments, "{}");
    }
}
