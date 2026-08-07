//! Headless check that the Rust client can actually reach Garmin.
//!
//! The open question this answers: Python's OpenSSL stack gets through
//! Cloudflare, but does Rust's? Reads tokens straight from the `garminconnect`
//! file (not the keyring) so it runs without any GUI or keyring prompt.
//!
//!   cargo run --bin spike

use anyhow::Result;
use garmin_core::{store, GarminClient, Tokens};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let path = store::python_token_path().expect("no home directory");
    println!("reading tokens from {}", path.display());

    let tokens = store::import_python_tokens(&path)?;
    println!("client_id: {}", tokens.di_client_id);
    match tokens.expires_in_secs() {
        Some(s) if s > 0 => println!("access token valid for {s}s"),
        Some(s) => println!("access token expired {}s ago — will refresh", -s),
        None => println!("access token unreadable — will refresh"),
    }

    // Write refreshed tokens back to the same file so repeat runs keep working
    // and the Python MCP setup stays in sync.
    let sink_path = path.clone();
    let on_change: Arc<dyn Fn(&Tokens) + Send + Sync> = Arc::new(move |t: &Tokens| {
        println!("  (tokens refreshed)");
        if let Ok(json) = serde_json::to_string_pretty(t) {
            let _ = std::fs::write(&sink_path, json);
        }
    });

    let client = GarminClient::new(tokens, on_change)?;

    let profile = client.profile().await?;
    println!("\nprofile: {}", profile.display_name);

    let acts = client.activities(0, 5).await?;
    println!("\n{} recent activities:", acts.len());
    for a in &acts {
        println!(
            "  [{}] {} — {} — {:.2} km, {:.0} min, avg HR {:?}, cadence {:?}",
            a.activity_id,
            a.start_time_local.as_deref().unwrap_or("?"),
            a.activity_name.as_deref().unwrap_or("?"),
            a.distance.unwrap_or(0.0) / 1000.0,
            a.duration.unwrap_or(0.0) / 60.0,
            a.average_hr,
            a.average_running_cadence_in_steps_per_minute,
        );
    }

    if let Some(latest) = acts.first() {
        println!("\nHR zones for {}:", latest.activity_id);
        let zones = client.hr_time_in_zones(latest.activity_id).await?;
        let total: f64 = zones.iter().filter_map(|z| z.secs_in_zone).sum();
        for z in &zones {
            let secs = z.secs_in_zone.unwrap_or(0.0);
            let pct = if total > 0.0 {
                secs / total * 100.0
            } else {
                0.0
            };
            println!(
                "  Z{}  {:>5.1} min  {:>5.1}%",
                z.zone_number,
                secs / 60.0,
                pct
            );
        }
    }

    println!("\nOK — Rust reaches Garmin.");
    Ok(())
}
