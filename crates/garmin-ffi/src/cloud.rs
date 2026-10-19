//! The cloud commands of the bridge: the hosted coach account.
//!
//! The mobile app shows the balance and the plan, and signs in by email code
//! for a premium account bought elsewhere. It has no checkout command: store
//! purchase comes later, and the worker's web checkout is for the desktop app.
//! No command returns a token. The UI sees only the `AccountView`.

use chrono::Utc;
use garmin_coach::{AccountView, Balance, CloudClient};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::ask::{bad, failed};
use crate::FfiError;

#[derive(Deserialize)]
struct Email {
    email: String,
}

#[derive(Deserialize)]
struct Verify {
    email: String,
    code: String,
}

/// Handle a `cloud_*` command, or return `None` for any other command.
pub(crate) async fn handle(
    cmd: &str,
    args: &Value,
    fixture: bool,
) -> Option<Result<String, FfiError>> {
    if !cmd.starts_with("cloud_") {
        return None;
    }
    if fixture {
        return Some(sample(cmd));
    }
    Some(live(cmd, args).await)
}

/// Sample mode touches no network and no secret store.
fn sample(cmd: &str) -> Result<String, FfiError> {
    match cmd {
        "cloud_account" => to_json(&sample_account()),
        "cloud_sign_out" => Ok("{}".into()),
        _ => Err(failed("Email sign-in is not available in sample mode.")),
    }
}

fn sample_account() -> AccountView {
    Balance {
        credits_remaining: 27,
        daily_grant: 30,
        ..Balance::default()
    }
    .view(Utc::now())
}

fn to_json(v: &impl serde::Serialize) -> Result<String, FfiError> {
    serde_json::to_string(v).map_err(failed)
}

async fn live(cmd: &str, args: &Value) -> Result<String, FfiError> {
    let cloud = CloudClient::default();
    match cmd {
        "cloud_account" => {
            // The coach page asks with `enrol`. The Ask tab only looks: it must
            // not use up a new install id for an app that is only open.
            let balance = if args["enrol"].as_bool() == Some(true) {
                Some(cloud.balance().await.map_err(failed)?)
            } else {
                cloud.balance_if_enrolled().await.map_err(failed)?
            };
            to_json(&balance.map(|b| b.view(Utc::now())))
        }
        "cloud_send_code" => {
            let a: Email = serde_json::from_value(args.clone()).map_err(bad)?;
            cloud.send_code(a.email.trim()).await.map_err(failed)?;
            Ok("{}".into())
        }
        "cloud_verify" => {
            let a: Verify = serde_json::from_value(args.clone()).map_err(bad)?;
            let email = cloud
                .verify_code(a.email.trim(), a.code.trim())
                .await
                .map_err(failed)?;
            Ok(json!({ "email": email }).to_string())
        }
        "cloud_sign_out" => {
            cloud.sign_out().await.map_err(failed)?;
            Ok("{}".into())
        }
        _ => Err(bad("unknown cloud command")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn only_cloud_commands_are_taken() {
        assert!(handle("today", &json!({}), true).await.is_none());
        assert!(handle("cloud_account", &json!({}), true).await.is_some());
    }

    #[tokio::test]
    async fn sample_mode_shows_a_sample_balance_and_signs_in_nowhere() {
        let out = handle("cloud_account", &json!({}), true)
            .await
            .unwrap()
            .unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["creditsLine"], "27 of 30 credits left today.");
        let sent = handle("cloud_send_code", &json!({"email":"a@b.c"}), true).await;
        assert!(matches!(sent, Some(Err(FfiError::Command { .. }))));
    }

    #[tokio::test]
    async fn a_verify_without_a_code_is_a_bad_argument() {
        let out = handle("cloud_verify", &json!({"email":"a@b.c"}), false)
            .await
            .unwrap();
        assert!(matches!(out, Err(FfiError::BadArgs { .. })));
    }
}
