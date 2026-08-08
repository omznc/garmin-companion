//! Headless sync into the local cache.
//!
//! The desktop app syncs from its Settings screen, but that needs a window and
//! a keyring unlock. This runs the identical `sync::sync_all` from a terminal,
//! reading tokens from the `garminconnect` file the way `spike` does, which
//! makes it the practical way to refresh the cache or verify a sync change.
//!
//!   cargo run --bin sync -- [days] [--full]

use anyhow::Result;
use garmin_core::{db::Db, store, sync, GarminClient, Tokens};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let full = args.iter().any(|a| a == "--full");
    let days: u32 = args.iter().find_map(|a| a.parse().ok()).unwrap_or(30);

    let path = store::python_token_path().expect("no home directory");
    let tokens = store::import_python_tokens(&path)?;

    // Persist refreshed tokens so repeat runs and the Python MCP setup stay
    // usable, same as the spike binary.
    let sink = path.clone();
    let on_change: Arc<dyn Fn(&Tokens) + Send + Sync> = Arc::new(move |t: &Tokens| {
        if let Ok(json) = serde_json::to_string_pretty(t) {
            let _ = std::fs::write(&sink, json);
        }
    });

    let client = GarminClient::new(tokens, on_change)?;
    let db = Db::open_default()?;

    println!("syncing {days} days{}…", if full { " (full)" } else { "" });
    // The same progress the app draws a bar from, one line per step — which is
    // also how a long sync gets watched from a terminal.
    let on = |p: sync::SyncProgress| {
        let of = p.total.map(|t| format!("/{t}")).unwrap_or_default();
        println!("  {:<10} {:>5}{of}  {}", p.phase, p.done, p.detail);
    };
    let report = sync::sync_all_with(&client, &db, days, full, &on).await?;

    println!(
        "activities: {} seen, {} written\ndays: {}\nworkouts: {}\ntracks: {}",
        report.activities_seen,
        report.activities_written,
        report.days_written,
        report.workouts_written,
        report.tracks_written,
    );
    if !report.warnings.is_empty() {
        println!("\n{} warning(s):", report.warnings.len());
        for w in report.warnings.iter().take(15) {
            println!("  {w}");
        }
    }
    Ok(())
}
