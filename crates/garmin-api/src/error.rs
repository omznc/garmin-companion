use serde::Serialize;

/// The one error type that crosses the API boundary.
///
/// It serializes as `{ "kind": ..., "message": ... }` so a bridge can show the
/// message and branch on the kind.
#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum ApiError {
    /// The command name is not known.
    #[error("unknown command: {0}")]
    UnknownCommand(String),
    /// The JSON arguments do not match the command.
    #[error("bad arguments: {0}")]
    BadArguments(String),
    /// The database or a query failed. The message holds the whole chain.
    #[error("{0}")]
    Internal(String),
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(
            e.chain()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(": "),
        )
    }
}
