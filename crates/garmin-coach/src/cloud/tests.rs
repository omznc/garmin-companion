//! Tests against a mock HTTP server on a local port. No test calls the live worker.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;
use crate::transport::{HttpTransport, Transport};
use crate::turn::{Event, EventSink};

const ID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const ID_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[derive(Debug, Clone)]
struct Req {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: String,
}

struct Resp {
    status: u16,
    headers: Vec<(&'static str, &'static str)>,
    body: String,
}

fn resp(status: u16, body: Value) -> Resp {
    Resp {
        status,
        headers: vec![("content-type", "application/json")],
        body: body.to_string(),
    }
}

fn error(status: u16, kind: &str) -> Resp {
    resp(
        status,
        json!({"error": {"message": "x", "code": status, "type": kind}}),
    )
}

fn sse(text: &str) -> Resp {
    let chunk = json!({"choices":[{"delta":{"content":text}}]});
    Resp {
        status: 200,
        headers: vec![("content-type", "text/event-stream")],
        body: format!("data: {chunk}\n\ndata: [DONE]\n\n"),
    }
}

type Seen = std::sync::Arc<Mutex<Vec<Req>>>;

/// Serve `handler` on a free port. Returns the base URL and the requests seen.
async fn mock(handler: impl Fn(&Req) -> Resp + Send + Sync + 'static) -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v2", listener.local_addr().unwrap());
    let seen: Seen = Seen::default();
    let log = seen.clone();
    let handler = std::sync::Arc::new(handler);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let (log, handler) = (log.clone(), handler.clone());
            tokio::spawn(async move {
                let req = read_request(&mut sock).await;
                let out = handler(&req);
                log.lock().unwrap().push(req);
                let mut head = format!(
                    "HTTP/1.1 {} X\r\ncontent-length: {}\r\nconnection: close\r\n",
                    out.status,
                    out.body.len()
                );
                for (k, v) in out.headers {
                    head.push_str(&format!("{k}: {v}\r\n"));
                }
                let _ = sock
                    .write_all(format!("{head}\r\n{}", out.body).as_bytes())
                    .await;
                let _ = sock.shutdown().await;
            });
        }
    });
    (base, seen)
}

async fn read_request(sock: &mut tokio::net::TcpStream) -> Req {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let (head_end, length) = loop {
        let n = sock.read(&mut chunk).await.unwrap();
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..i]).to_lowercase();
            let length = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            break (i + 4, length);
        }
        assert!(n > 0, "the client closed early");
    };
    while buf.len() < head_end + length {
        let n = sock.read(&mut chunk).await.unwrap();
        buf.extend_from_slice(&chunk[..n]);
    }
    let text = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = text.lines();
    let mut first = lines.next().unwrap().split(' ');
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string()))
        .collect();
    Req {
        method: first.next().unwrap().into(),
        path: first.next().unwrap().into(),
        headers,
        body: String::from_utf8_lossy(&buf[head_end..head_end + length]).into(),
    }
}

fn client(base: &str) -> (CloudClient, Arc<MemoryVault>) {
    let vault = Arc::new(MemoryVault::default());
    (CloudClient::new(base, vault.clone()), vault)
}

fn bearer(r: &Req) -> &str {
    r.headers["authorization"].trim_start_matches("Bearer ")
}

fn balance_json(left: u64) -> Value {
    json!({"plan":"free","premium":false,"email":null,"credits_remaining":left,
        "daily_grant":30,"resets_at":"2026-10-19T00:00:00.000Z","month_used":0,"month_ceiling":6000})
}

#[derive(Default)]
struct Sink(Mutex<String>);
impl EventSink for Sink {
    fn emit(&self, e: Event) {
        if let Event::Delta { text } = e {
            self.0.lock().unwrap().push_str(&text);
        }
    }
}

#[tokio::test]
async fn the_first_call_enrols_and_keeps_the_install_id() {
    let (base, seen) = mock(|r| match r.path.as_str() {
        "/v2/install" => resp(200, json!({ "id": ID_A })),
        _ => resp(200, balance_json(27)),
    })
    .await;
    let (c, vault) = client(&base);
    let b = c.balance().await.unwrap();
    assert_eq!(b.credits_remaining, 27);
    assert_eq!(vault.install_id().unwrap().as_deref(), Some(ID_A));
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[1].path, "/v2/balance");
    assert_eq!(bearer(&seen[1]), ID_A);
}

#[tokio::test]
async fn a_session_is_sent_in_place_of_the_install_id() {
    let (base, seen) = mock(|_| resp(200, balance_json(400))).await;
    let (c, vault) = client(&base);
    vault.save_install_id(ID_A).unwrap();
    vault.save_session("s_token").unwrap();
    c.balance().await.unwrap();
    assert_eq!(bearer(&seen.lock().unwrap()[0]), "s_token");
}

#[tokio::test]
async fn a_chat_streams_and_each_request_has_its_own_id() {
    let (base, seen) = mock(|r| match r.path.as_str() {
        "/v2/install" => resp(200, json!({ "id": ID_A })),
        _ => sse("Hello"),
    })
    .await;
    let (c, _) = client(&base);
    let t = HttpTransport::cloud(c);
    let sink = Sink::default();
    let msgs = [json!({"role":"user","content":"hi"})];
    for _ in 0..2 {
        let out = t
            .complete(&msgs, None, &sink, &AtomicBool::new(false))
            .await;
        assert_eq!(out.unwrap().content, "Hello");
    }
    let seen = seen.lock().unwrap();
    let chats: Vec<_> = seen
        .iter()
        .filter(|r| r.path.ends_with("completions"))
        .collect();
    let ids: Vec<_> = chats
        .iter()
        .map(|r| r.headers["x-request-id"].clone())
        .collect();
    assert_ne!(ids[0], ids[1]);
    assert!((8..=64).contains(&ids[0].len()));
    let body: Value = serde_json::from_str(&chats[0].body).unwrap();
    assert_eq!(body["stream"], true);
    assert!(body.get("model").is_none(), "the worker sets the model");
}

#[tokio::test]
async fn an_unknown_install_is_issued_again_once() {
    let (base, seen) = mock(|r| match (r.path.as_str(), bearer_or_empty(r)) {
        ("/v2/install", _) => resp(200, json!({ "id": ID_B })),
        (_, ID_A) => error(401, "unknown_install"),
        _ => sse("ok"),
    })
    .await;
    let (c, vault) = client(&base);
    vault.save_install_id(ID_A).unwrap();
    let t = HttpTransport::cloud(c);
    let msgs = [json!({"role":"user","content":"hi"})];
    let out = t
        .complete(&msgs, None, &Sink::default(), &AtomicBool::new(false))
        .await
        .unwrap();
    assert_eq!(out.content, "ok");
    assert_eq!(vault.install_id().unwrap().as_deref(), Some(ID_B));
    assert_eq!(seen.lock().unwrap().len(), 3);
}

fn bearer_or_empty(r: &Req) -> &str {
    r.headers
        .get("authorization")
        .map_or("", |v| v.trim_start_matches("Bearer "))
}

#[tokio::test]
async fn out_of_credits_reads_as_a_sentence_with_the_reset_time() {
    let (base, _) = mock(|_| {
        resp(
            402,
            json!({"error":{"message":"x","code":402,"type":"out_of_credits","resets_at":"2099-01-01T00:00:00Z"}}),
        )
    })
    .await;
    let (c, vault) = client(&base);
    vault.save_install_id(ID_A).unwrap();
    let e = c.chat(&json!({"messages":[]})).await.unwrap_err();
    assert_eq!(error_kind(&e), Some("out_of_credits"));
    assert!(e
        .to_string()
        .starts_with("You are out of credits. They return in "));
    assert!(!e.to_string().contains('{'));
}

#[tokio::test]
async fn a_rate_limit_names_the_wait() {
    let (base, _) = mock(|_| {
        let mut r = error(429, "rate_limited");
        r.headers.push(("retry-after", "12"));
        r
    })
    .await;
    let (c, vault) = client(&base);
    vault.save_install_id(ID_A).unwrap();
    let e = c.balance().await.unwrap_err();
    assert!(e.to_string().contains("Wait 12 seconds"));
}

#[tokio::test]
async fn an_error_chunk_in_a_200_stream_fails_the_call() {
    let (base, _) = mock(|_| Resp {
        status: 200,
        headers: vec![],
        body: format!("data: {}\n\n", json!({"error":{"message":"model down"}})),
    })
    .await;
    let (c, vault) = client(&base);
    vault.save_install_id(ID_A).unwrap();
    let msgs = [json!({"role":"user","content":"hi"})];
    let out = HttpTransport::cloud(c)
        .complete(&msgs, None, &Sink::default(), &AtomicBool::new(false))
        .await;
    assert!(out.unwrap_err().to_string().contains("model down"));
}

#[tokio::test]
async fn mail_not_set_up_says_so_in_plain_words() {
    let (base, _) = mock(|_| error(503, "mail_unavailable")).await;
    let (c, _) = client(&base);
    let e = c.send_code("a@b.com").await.unwrap_err();
    assert_eq!(e.to_string(), "Email sign-in is not available yet.");
}

#[tokio::test]
async fn verify_stores_the_session_and_sign_out_drops_it() {
    let token = format!("s_{}", "ab".repeat(32));
    let t = token.clone();
    let (base, seen) = mock(move |r| match r.path.as_str() {
        "/v2/auth/email/verify" => resp(200, json!({"token": t, "email": "a@b.com"})),
        _ => resp(200, json!({"ok": true})),
    })
    .await;
    let (c, vault) = client(&base);
    assert_eq!(c.verify_code("a@b.com", "123456").await.unwrap(), "a@b.com");
    assert_eq!(vault.session().unwrap(), Some(token.clone()));
    c.sign_out().await.unwrap();
    assert_eq!(vault.session().unwrap(), None);
    let seen = seen.lock().unwrap();
    assert_eq!(bearer(&seen[1]), token);
}

#[tokio::test]
async fn a_checkout_link_comes_back() {
    let (base, seen) = mock(|_| resp(200, json!({"url":"https://polar.example/c"}))).await;
    let (c, vault) = client(&base);
    vault.save_session("s_x").unwrap();
    assert_eq!(c.checkout_url().await.unwrap(), "https://polar.example/c");
    assert_eq!(seen.lock().unwrap()[0].path, "/v2/billing/checkout");
}

#[tokio::test]
async fn a_413_trims_old_tool_results_and_tries_once_more() {
    let (base, seen) = mock(|r| {
        if r.body.len() > 3000 {
            error(413, "context_too_large")
        } else {
            sse("ok")
        }
    })
    .await;
    let (c, vault) = client(&base);
    vault.save_install_id(ID_A).unwrap();
    let big = "x".repeat(2000);
    let msgs = vec![
        json!({"role":"user","content":"q"}),
        json!({"role":"tool","content": big}),
        json!({"role":"tool","content": big}),
    ];
    // The limit halves from 100,000 to 50,000: the mock's own cap is smaller,
    // so this checks the retry path and its end, not the final size.
    let r = c.chat(&json!({ "messages": msgs })).await;
    assert_eq!(error_kind(&r.unwrap_err()), Some("context_too_large"));
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[test]
fn fit_drops_the_oldest_tool_results_and_keeps_the_newest() {
    let big = "x".repeat(300);
    let mut body = json!({"messages":[
        {"role":"user","content":"q"},
        {"role":"tool","content": big},
        {"role":"tool","content": big},
        {"role":"tool","content": big},
    ]});
    fit(&mut body, 700);
    let m = body["messages"].as_array().unwrap();
    assert_eq!(m[1]["content"], LEFT_OUT);
    assert_eq!(m[2]["content"], LEFT_OUT);
    assert_eq!(m[3]["content"], big);
    assert_eq!(m[0]["content"], "q");
}

#[test]
fn the_reset_time_reads_in_hours_and_minutes() {
    let now = DateTime::parse_from_rfc3339("2026-10-18T18:48:00Z")
        .unwrap()
        .into();
    assert_eq!(until("2026-10-19T00:00:00.000Z", now), "in 5 h 12 min");
    assert_eq!(until("2026-10-18T19:00:00Z", now), "in 12 min");
    assert_eq!(until("2026-10-18T10:00:00Z", now), "soon");
    assert_eq!(until("nonsense", now), "at the next reset");
}

#[test]
fn the_balance_becomes_lines_and_flags() {
    let now = Utc::now();
    let mut b: Balance = serde_json::from_value(balance_json(27)).unwrap();
    let v = b.view(now);
    assert_eq!(v.credits_line, "27 of 30 credits left today.");
    assert_eq!(v.plan_line, "Free plan: 30 credits a day.");
    assert!(!v.out && !v.low);
    b.credits_remaining = 5;
    assert!(b.view(now).low);
    b.credits_remaining = 0;
    assert!(b.view(now).out);
    b.premium = true;
    b.credits_remaining = 300;
    b.email = Some("a@b.com".into());
    b.renews_at = Some("2026-11-18T00:00:00Z".into());
    b.cancels_at_period_end = true;
    let v = b.view(now);
    assert_eq!(v.plan_line, "Premium for a@b.com. It ends on 2026-11-18.");
    assert_eq!(
        v.credits_line,
        "300 credits left today. 0 of 6000 used this month."
    );
}
