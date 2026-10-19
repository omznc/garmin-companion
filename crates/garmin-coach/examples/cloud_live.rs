//! A manual check of the live cloud coach: one tiny streamed question.
//!
//! It keeps the install id in memory, so it leaves nothing in the keyring.
//! It costs about one credit. Run it by hand, never in a test:
//! `cargo run -p garmin-coach --example cloud_live`.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use garmin_coach::{
    CloudClient, Event, EventSink, HttpTransport, MemoryVault, Transport, CLOUD_BASE,
};
use serde_json::json;

struct Print;

impl EventSink for Print {
    fn emit(&self, event: Event) {
        if let Event::Delta { text } = event {
            print!("{text}");
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cloud = CloudClient::new(CLOUD_BASE, Arc::new(MemoryVault::default()));
    let before = cloud.balance().await?.view(chrono::Utc::now());
    println!("before: {}", before.credits_line);

    let transport = HttpTransport::cloud(cloud.clone());
    let messages = [json!({ "role": "user", "content": "Reply with the single word: ready." })];
    print!("answer: ");
    let out = transport
        .complete(&messages, None, None, &Print, &AtomicBool::new(false))
        .await?;
    println!("\nchars: {}", out.content.len());

    let after = cloud.balance().await?.view(chrono::Utc::now());
    println!("after: {}", after.credits_line);
    Ok(())
}
