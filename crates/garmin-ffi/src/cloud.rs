//! The cloud commands of the bridge: the hosted coach account.
//!
//! The mobile app shows the balance and the plan, and signs in by email code
//! for a premium account bought elsewhere. It has no checkout command: store
//! purchase comes later, and the worker's web checkout is for the desktop app.
//! No command returns a token. The UI sees only the `AccountView`.

use chrono::Utc;
use garmin_coach::{AccountView, Balance, CloudClient};
use serde::Deserialize;
use serde_json::Value;

use crate::ask::{bad, failed};
use crate::FfiError;

/// The arguments of `cloud_account`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub(crate) struct CloudAccountArgs {
    /// Ask the worker for a balance, and enrol this install if needed. The
    /// Ask tab only looks, so it leaves this out.
    #[cfg_attr(feature = "ts-rs", ts(optional))]
    enrol: Option<bool>,
}

/// The arguments of `cloud_send_code`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub(crate) struct CloudEmailArgs {
    email: String,
}

/// The arguments of `cloud_verify`.
#[derive(Deserialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub(crate) struct CloudVerifyArgs {
    email: String,
    code: String,
}

/// The result of `cloud_verify`: the email that the code proved.
#[derive(serde::Serialize)]
#[cfg_attr(feature = "ts-rs", derive(ts_rs::TS), ts(export))]
pub(crate) struct CloudVerified {
    email: String,
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
            let a: CloudAccountArgs = serde_json::from_value(args.clone()).map_err(bad)?;
            let balance = if a.enrol == Some(true) {
                Some(cloud.balance().await.map_err(failed)?)
            } else {
                cloud.balance_if_enrolled().await.map_err(failed)?
            };
            to_json(&balance.map(|b| b.view(Utc::now())))
        }
        "cloud_send_code" => {
            let a: CloudEmailArgs = serde_json::from_value(args.clone()).map_err(bad)?;
            cloud.send_code(a.email.trim()).await.map_err(failed)?;
            Ok("{}".into())
        }
        "cloud_verify" => {
            let a: CloudVerifyArgs = serde_json::from_value(args.clone()).map_err(bad)?;
            let email = cloud
                .verify_code(a.email.trim(), a.code.trim())
                .await
                .map_err(failed)?;
            to_json(&CloudVerified { email })
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
    use serde_json::json;

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
    async fn an_account_with_a_bad_enrol_is_a_bad_argument() {
        // The arguments fail to parse before any keyring or network use.
        let out = handle("cloud_account", &json!({"enrol":"x"}), false)
            .await
            .unwrap();
        assert!(matches!(out, Err(FfiError::BadArgs { .. })));
    }

    #[tokio::test]
    async fn a_verify_without_a_code_is_a_bad_argument() {
        let out = handle("cloud_verify", &json!({"email":"a@b.c"}), false)
            .await
            .unwrap();
        assert!(matches!(out, Err(FfiError::BadArgs { .. })));
    }
}
