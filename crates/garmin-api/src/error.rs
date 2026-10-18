use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

/// The one error type that crosses the API boundary.
///
/// It serializes as `{ "kind": ..., "message": ... }` so a bridge can show the
/// message and branch on the kind. Every kind carries a message, built from
/// the error text.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// The command name is not known.
    #[error("unknown command: {0}")]
    UnknownCommand(String),
    /// The JSON arguments do not match the command.
    #[error("bad arguments: {0}")]
    BadArguments(String),
    /// No Garmin account is signed in.
    #[error("Not connected to Garmin yet.")]
    NotConnected,
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

impl ApiError {
    fn kind(&self) -> &'static str {
        match self {
            Self::UnknownCommand(_) => "unknown_command",
            Self::BadArguments(_) => "bad_arguments",
            Self::NotConnected => "not_connected",
            Self::Internal(_) => "internal",
        }
    }
}

impl Serialize for ApiError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_struct("ApiError", 2)?;
        out.serialize_field("kind", self.kind())?;
        out.serialize_field("message", &self.to_string())?;
        out.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_readable_message() {
        let not_connected = serde_json::to_value(ApiError::NotConnected).unwrap();
        assert_eq!(not_connected["kind"], "not_connected");
        assert_eq!(not_connected["message"], "Not connected to Garmin yet.");
        let internal = serde_json::to_value(ApiError::Internal("disk".into())).unwrap();
        assert_eq!(internal["message"], "disk");
    }
}
