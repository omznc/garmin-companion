//! Garmin Connect MCP server (stdio).
//!
//! Replaces the Python `garmin_mcp` extension. Shares its Garmin client, token
//! store and cache with the desktop app via `garmin-core`, so connecting an
//! account in one connects it in the other.

mod server;

use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};

const USAGE: &str = "\
garmin-mcp — Garmin Connect MCP server

  garmin-mcp                     run the MCP server on stdio (default)
  garmin-mcp import [PATH]       adopt tokens from a garminconnect token file
                                 (default: ~/.garminconnect/garmin_tokens.json)
  garmin-mcp status              show connection and cache state
  garmin-mcp sync [DAYS] [--full]
                                 refresh the local cache (default: 30 days).
                                 --full walks the whole activity history
                                 instead of stopping once caught up.
  garmin-mcp coach               what the coach would say today, and the week
                                 against the goals. Exits 0 with nothing to say
                                 when there is nothing to say.
";

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        None => serve().await,
        Some("import") => import(args.get(1).map(String::as_str)),
        Some("status") => status(),
        Some("sync") => {
            let full = args.iter().any(|a| a == "--full");
            let days = args
                .iter()
                .skip(1)
                .find(|a| !a.starts_with('-'))
                .and_then(|d| d.parse().ok())
                .unwrap_or(30);
            sync(days, full).await
        }
        Some("coach") => coach(),
        Some("-h" | "--help" | "help") => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => {
            eprintln!("unknown command: {other}\n\n{USAGE}");
            std::process::exit(2);
        }
    }
}

async fn serve() -> Result<()> {
    // stdout is the MCP transport — every diagnostic must go to stderr or it
    // corrupts the protocol stream.
    eprintln!("garmin-mcp {} starting", env!("CARGO_PKG_VERSION"));

    let service = server::GarminServer.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

/// Adopt tokens from a `garminconnect` token file into the OS keyring, so the
/// MCP server can be set up without launching the desktop app.
fn import(path: Option<&str>) -> Result<()> {
    let path = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => garmin_core::store::python_token_path()
            .ok_or_else(|| anyhow::anyhow!("could not locate a home directory"))?,
    };

    let tokens = garmin_core::store::import_python_tokens(&path)?;
    garmin_core::store::save_tokens(&tokens)?;
    println!("Imported tokens from {}", path.display());
    println!("Client: {}", tokens.di_client_id);
    match tokens.expires_in_secs() {
        Some(s) if s > 0 => println!("Access token valid for {} min", s / 60),
        _ => println!("Access token expired — it'll refresh on first use"),
    }
    Ok(())
}

fn status() -> Result<()> {
    let db = garmin_core::Db::open_default()?;
    println!(
        "Connected:  {}",
        garmin_core::store::load_tokens()?.is_some()
    );
    println!("Activities: {}", db.activity_count()?);
    println!(
        "Last sync:  {}",
        db.sync_state("last_sync")?
            .unwrap_or_else(|| "never".into())
    );
    if let Some(p) = garmin_core::db::default_path() {
        println!("Cache:      {}", p.display());
    }
    Ok(())
}

/// Print the week and anything the coach has to say.
///
/// Reads the cache only — it never syncs, so a cron job should run `sync` first.
fn coach() -> Result<()> {
    let db = garmin_core::Db::open_default()?;
    let today = chrono::Local::now().date_naive();
    let report = garmin_core::coach::for_today(&db, today)?;

    println!("Week of {}", report.week.week_start);
    println!(
        "  {} sessions, {:.0} min, longest run {:.0} min",
        report.week.sessions, report.week.minutes, report.week.longest_run_minutes
    );
    for ring in &report.week.rings {
        println!(
            "  {:14} {:>6} / {:<6} {:<8} {}{}",
            ring.label,
            ring.actual,
            ring.target,
            ring.unit,
            if ring.met { "met" } else { "" },
            if ring.thin { "  (thin data)" } else { "" },
        );
    }

    if report.nudges.is_empty() {
        println!("\nNothing worth raising today.");
        return Ok(());
    }

    for n in &report.nudges {
        println!(
            "\n[{:?}] {}{}",
            n.nudge.tone,
            n.nudge.title,
            if n.days_running > 1 {
                format!("  (day {} of saying this)", n.days_running)
            } else {
                String::new()
            }
        );
        println!("  {}", n.nudge.body);
        for e in &n.nudge.evidence {
            println!("    · {e}");
        }
    }
    Ok(())
}

async fn sync(days: u32, full: bool) -> Result<()> {
    let client = garmin_core::require_client()?;
    let db = garmin_core::Db::open_default()?;
    let report = garmin_core::sync::sync_all(&client, &db, days, full).await?;

    println!(
        "Activities: {} seen, {} new/updated",
        report.activities_seen, report.activities_written
    );
    println!("Days:       {} written", report.days_written);
    for w in &report.warnings {
        eprintln!("  warning: {w}");
    }
    Ok(())
}
