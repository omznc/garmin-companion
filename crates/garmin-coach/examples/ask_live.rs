//! A manual check of full Ask turns (tools included) on the live cloud coach,
//! against the local cache. The install id stays in memory, so each run takes
//! a new one, and the worker allows 10 new ids a day per network.
//! `cargo run -p garmin-coach --example ask_live -- "question" ["question" ...]`

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use garmin_coach::{
    run_turn, CloudClient, Event, EventSink, HistoryMessage, HttpTransport, MemoryVault, CLOUD_BASE,
};
use garmin_core::Db;

struct Print(Instant);

impl EventSink for Print {
    fn emit(&self, event: Event) {
        let t = self.0.elapsed().as_secs_f32();
        match event {
            // GC_ANSWER=1 prints the streamed text too.
            Event::Delta { text } if std::env::var_os("GC_ANSWER").is_some() => print!("{text}"),
            Event::Delta { .. } => {}
            Event::Tool {
                label, running, ok, ..
            } => {
                println!("{t:6.1}s tool {label} running={running} ok={ok}")
            }
            Event::Draft { draft, .. } => println!(
                "{t:6.1}s DRAFT {} ({:?}, {} steps)",
                draft.name,
                draft.sport,
                draft.flat_count()
            ),
            Event::Note { text } => println!("{t:6.1}s note {} chars", text.len()),
            Event::Done { sources } => println!("{t:6.1}s DONE sources={sources:?}"),
            Event::Error { text } => println!("{t:6.1}s ERROR {text}"),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cloud = CloudClient::new(CLOUD_BASE, Arc::new(MemoryVault::default()));
    let transport = HttpTransport::cloud(cloud.clone());
    for q in std::env::args().skip(1) {
        println!("== {q}");
        let history = vec![HistoryMessage {
            role: "user".into(),
            content: q,
        }];
        let sink = Print(Instant::now());
        let _ = run_turn(
            &transport,
            &sink,
            &AtomicBool::new(false),
            history,
            &Db::open_default,
            true,
        )
        .await;
    }
    let after = cloud.balance().await?.view(chrono::Utc::now());
    println!("credits: {}", after.credits_line);
    Ok(())
}
