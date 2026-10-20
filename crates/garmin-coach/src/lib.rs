//! The AI coach chat engine, shared by the desktop and the mobile app.
//!
//! The model answers by calling tools that read the local cache. It never
//! gets the cache whole, and it has no network path to Garmin. Only the rows
//! that a tool returns for the question leave the device.
//!
//! Streamed tokens and tool steps go out through an [`EventSink`]. The host
//! decides what a sink does: the desktop app sends them to a view, and the
//! mobile bridge sends them as JSON events.
//!
//! The Tauri crate has its own older copy of this engine. That app is frozen.

mod cloud;
mod config;
mod prompt;
mod sse;
mod tools;
mod transport;
mod turn;

pub use cloud::{
    error_kind, until, AccountView, Balance, CloudClient, CloudError, KeyringVault, MemoryVault,
    Vault, CLOUD_BASE,
};
pub use config::{
    load_config, probe_ollama, save_config, ChatConfig, Provider, DEFAULT_OPENROUTER_MODEL,
};
pub use sse::{SseParser, StreamResult, ToolCall};
pub use tools::Period;
pub use transport::{HttpTransport, Transport};
pub use turn::{ask, run_turn, run_turn_in, Event, EventSink, HistoryMessage};
