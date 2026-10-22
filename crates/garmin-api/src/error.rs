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
    /// The network to Garmin is down or blocked, for example when the app runs in the background.
    #[error("No connection to Garmin.")]
    Offline,
    /// The database or a query failed. The message holds the whole chain.
    #[error("{0}")]
    Internal(String),
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        if e.chain().any(is_offline) {
            return Self::Offline;
        }
        Self::Internal(
            e.chain()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(": "),
        )
    }
}

fn is_offline(cause: &(dyn std::error::Error + 'static)) -> bool {
    cause
        .downcast_ref::<reqwest::Error>()
        .is_some_and(|r| r.is_connect() || r.is_timeout())
}

impl ApiError {
    fn kind(&self) -> &'static str {
        match self {
            Self::UnknownCommand(_) => "unknown_command",
            Self::BadArguments(_) => "bad_arguments",
            Self::NotConnected => "not_connected",
            Self::Offline => "offline",
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
        let offline = serde_json::to_value(ApiError::Offline).unwrap();
        assert_eq!(offline["kind"], "offline");
        assert_eq!(offline["message"], "No connection to Garmin.");
    }

    #[test]
    fn a_connect_error_in_the_chain_is_offline() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let connect = rt
            .block_on(reqwest::Client::new().get("http://127.0.0.1:1/").send())
            .unwrap_err();
        let e = anyhow::Error::new(connect).context("track 123");
        assert!(matches!(ApiError::from(e), ApiError::Offline));
        let other = anyhow::anyhow!("disk full");
        assert!(matches!(ApiError::from(other), ApiError::Internal(_)));
    }
}
