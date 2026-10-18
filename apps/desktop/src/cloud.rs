//! The coach settings: where the model runs, the cloud account, the sign-in.
//!
//! The state lives in `Account::cloud`. A call to the worker runs on its own
//! thread with a small tokio runtime, like the Ask turn does, and the result
//! comes back to the view through a channel. The view never sees a token: the
//! core sends an `AccountView` with the sentences already built.
//!
//! In a persona demo nothing leaves the computer: the numbers are a sample, and
//! the actions say so.

use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;
use garmin_coach::{AccountView, Balance, CloudClient, Provider};
use garmin_core::{db::Db, store};
use gpui::{div, prelude::*, px, relative, App, ClickEvent, Context, Div, FontWeight, Window};

use crate::controls::quiet;
use crate::data::Source;
use crate::field::Field;
use crate::settings::{prose, section};
use crate::shell::Root;
use crate::theme::{Theme, FS_CAPTION, FS_MD, FS_SMALL};
use crate::ui::{body, ch};

/// How often the checkout page is watched, and for how long (2 minutes).
const POLL_EVERY: Duration = Duration::from_secs(3);
const POLL_TIMES: usize = 40;

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub text: String,
    pub bad: bool,
}

pub struct Cloud {
    /// False in a persona demo: no network, no secret store, no database.
    pub live: bool,
    /// The cache that holds the provider choice. `None` is the data folder.
    config_db: Option<PathBuf>,
    pub provider: Provider,
    pub view: Option<AccountView>,
    pub note: Option<Note>,
    /// The email that a code went to. `None` while no code is waiting.
    pending_email: Option<String>,
    busy: bool,
    has_key: bool,
    email: Field,
    code: Field,
    key: Field,
}

impl Cloud {
    pub fn new(cx: &mut App) -> Self {
        Self {
            live: false,
            config_db: None,
            provider: Provider::Cloud,
            view: None,
            note: None,
            pending_email: None,
            busy: false,
            has_key: false,
            email: Field::new("you@example.com", false, cx),
            code: Field::new("6-digit code", false, cx),
            key: Field::new("OpenRouter API key", true, cx),
        }
    }

    pub fn say(&mut self, text: impl Into<String>, bad: bool) {
        self.note = Some(Note {
            text: text.into(),
            bad,
        });
    }
}

/* ------------------------------------------------------------ the network --- */

/// Run `job` on its own thread with its own runtime, and wait for the result.
pub async fn on_thread<T: Send + 'static>(
    job: impl Future<Output = Result<T, String>> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        let out = match runtime {
            Ok(rt) => rt.block_on(job),
            Err(_) => Err("Could not start the network runtime.".to_string()),
        };
        let _ = tx.send_blocking(out);
    });
    rx.recv()
        .await
        .unwrap_or_else(|_| Err("The task stopped without a result.".into()))
}

fn text<T>(r: anyhow::Result<T>) -> Result<T, String> {
    r.map_err(|e| format!("{e:#}"))
}

pub async fn account() -> Result<AccountView, String> {
    text(CloudClient::default().balance().await).map(|b: Balance| b.view(Utc::now()))
}

/// What the sample view says in a persona demo.
pub fn sample_account() -> AccountView {
    Balance {
        credits_remaining: 27,
        daily_grant: 30,
        ..Balance::default()
    }
    .view(Utc::now())
}

/* ---------------------------------------------------------------- actions --- */

impl Root {
    /// The Settings screen was opened: read the provider and the balance.
    pub fn cloud_open(&mut self, cx: &mut Context<Self>) {
        let (live, path) = match &self.source {
            Source::Persona(_) => (false, None),
            Source::Db { path, .. } => (true, Some(path.clone())),
            Source::Default => (true, None),
        };
        let cloud = &mut self.account.cloud;
        cloud.live = live;
        cloud.config_db = path;
        if live {
            cloud.provider = cloud.read_provider().unwrap_or(Provider::Cloud);
            cloud.has_key = matches!(store::load_openrouter_key(), Ok(Some(_)));
        }
        if !live {
            cloud.view = Some(sample_account());
        } else if cloud.provider == Provider::Cloud {
            self.cloud_refresh(cx);
        }
    }

    pub fn cloud_refresh(&mut self, cx: &mut Context<Self>) {
        if !self.account.cloud.live {
            return;
        }
        self.cloud_call(cx, account(), |root, out, _| match out {
            Ok(view) => root.account.cloud.view = Some(view),
            Err(e) => root.account.cloud.say(e, true),
        });
    }

    /// Run a job, mark the state busy until it ends, then hand the result over.
    fn cloud_call<T: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        job: impl Future<Output = Result<T, String>> + Send + 'static,
        done: impl FnOnce(&mut Root, Result<T, String>, &mut Context<Root>) + 'static,
    ) {
        self.account.cloud.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let out = on_thread(job).await;
            let _ = this.update(cx, |root, cx| {
                root.account.cloud.busy = false;
                done(root, out, cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn cloud_choose(&mut self, provider: Provider, cx: &mut Context<Self>) {
        let cloud = &mut self.account.cloud;
        cloud.note = None;
        if provider == Provider::Ollama && cloud.live {
            return self.cloud_choose_ollama(cx);
        }
        self.cloud_keep(provider, "", cx);
    }

    /// Save the choice. A persona demo keeps it in memory only.
    fn cloud_keep(&mut self, provider: Provider, model: &str, cx: &mut Context<Self>) {
        let cloud = &mut self.account.cloud;
        if cloud.live {
            if let Err(e) = cloud.write_provider(provider, model) {
                cloud.say(format!("Could not save the choice: {e:#}"), true);
                return;
            }
        }
        cloud.provider = provider;
        if provider == Provider::Cloud {
            self.cloud_refresh(cx);
        }
        cx.notify();
    }

    fn cloud_choose_ollama(&mut self, cx: &mut Context<Self>) {
        let probe = async {
            let http = reqwest::Client::new();
            Ok(garmin_coach::probe_ollama(&http).await)
        };
        self.cloud_call(cx, probe, |root, out, cx| match out {
            Ok((true, models)) if !models.is_empty() => {
                root.cloud_keep(Provider::Ollama, &models[0], cx)
            }
            _ => root
                .account
                .cloud
                .say("Ollama is not running here, or it has no model.", true),
        });
    }

    pub fn cloud_save_key(&mut self, cx: &mut Context<Self>) {
        let cloud = &mut self.account.cloud;
        let key = cloud.key.line.text().trim().to_string();
        if key.is_empty() || !cloud.live {
            return;
        }
        match store::save_openrouter_key(&key) {
            Ok(()) => {
                cloud.key.line.clear();
                cloud.has_key = true;
                cloud.say("Key saved in the keyring of this computer.", false);
                self.cloud_keep(
                    Provider::Openrouter,
                    garmin_coach::DEFAULT_OPENROUTER_MODEL,
                    cx,
                );
            }
            Err(e) => cloud.say(format!("Could not save the key: {e:#}"), true),
        }
    }

    pub fn cloud_send_code(&mut self, cx: &mut Context<Self>) {
        let cloud = &mut self.account.cloud;
        let email = cloud.email.line.text().trim().to_string();
        if email.is_empty() || cloud.busy {
            return;
        }
        if !cloud.live {
            return cloud.say("Sign-in is off in a demo.", true);
        }
        cloud.note = None;
        let sent = email.clone();
        let job = async move { text(CloudClient::default().send_code(&sent).await) };
        self.cloud_call(cx, job, move |root, out, _| {
            let cloud = &mut root.account.cloud;
            match out {
                Ok(()) => {
                    cloud.say("A code is on its way. It works for 10 minutes.", false);
                    cloud.pending_email = Some(email);
                }
                Err(e) => cloud.say(e, true),
            }
        });
    }

    pub fn cloud_verify(&mut self, cx: &mut Context<Self>) {
        let cloud = &mut self.account.cloud;
        let code = cloud.code.line.text().trim().to_string();
        let Some(email) = cloud.pending_email.clone() else {
            return;
        };
        if code.is_empty() || cloud.busy {
            return;
        }
        cloud.note = None;
        let job = async move { text(CloudClient::default().verify_code(&email, &code).await) };
        self.cloud_call(cx, job, |root, out, cx| match out {
            Ok(email) => {
                let cloud = &mut root.account.cloud;
                cloud.code.line.clear();
                cloud.email.line.clear();
                cloud.pending_email = None;
                cloud.say(format!("Signed in as {email}."), false);
                root.cloud_refresh(cx);
            }
            Err(e) => root.account.cloud.say(e, true),
        });
    }

    pub fn cloud_sign_out(&mut self, cx: &mut Context<Self>) {
        if !self.account.cloud.live {
            return;
        }
        let job = async { text(CloudClient::default().sign_out().await) };
        self.cloud_call(cx, job, |root, out, cx| match out {
            Ok(()) => {
                root.account.cloud.say("Signed out.", false);
                root.cloud_refresh(cx);
            }
            Err(e) => root.account.cloud.say(e, true),
        });
    }

    /// Open the Polar checkout, or ask for the sign-in that it needs.
    pub fn cloud_checkout(&mut self, cx: &mut Context<Self>) {
        let cloud = &mut self.account.cloud;
        if !cloud.live {
            return cloud.say("Checkout is off in a demo.", true);
        }
        if cloud.view.as_ref().is_some_and(|v| v.email.is_none()) {
            return cloud.say(
                "Sign in with your email first. Premium belongs to an account.",
                true,
            );
        }
        let job = async { text(CloudClient::default().checkout_url().await) };
        self.cloud_call(cx, job, |root, out, cx| match out {
            Ok(url) => {
                cx.open_url(&url);
                root.account.cloud.say(
                    "The checkout opened in your browser. This page updates when the payment is done.",
                    false,
                );
                root.cloud_watch(cx);
            }
            Err(e) => root.account.cloud.say(e, true),
        });
    }

    /// The customer portal: cancel, card, invoices.
    pub fn cloud_portal(&mut self, cx: &mut Context<Self>) {
        if !self.account.cloud.live {
            return;
        }
        let job = async { text(CloudClient::default().portal_url().await) };
        self.cloud_call(cx, job, |root, out, cx| match out {
            Ok(url) => cx.open_url(&url),
            Err(e) => root.account.cloud.say(e, true),
        });
    }

    /// Read the balance every 3 seconds for 2 minutes, until premium shows.
    fn cloud_watch(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            for _ in 0..POLL_TIMES {
                cx.background_executor().timer(POLL_EVERY).await;
                let Ok(view) = on_thread(account()).await else {
                    continue;
                };
                let premium = view.premium;
                let alive = this.update(cx, |root, cx| {
                    if premium {
                        root.account
                            .cloud
                            .say("Premium is active. Thank you.", false);
                    }
                    root.account.cloud.view = Some(view);
                    cx.notify();
                });
                if alive.is_err() || premium {
                    break;
                }
            }
        })
        .detach();
    }
}

impl Cloud {
    fn db(&self) -> anyhow::Result<Db> {
        match &self.config_db {
            Some(path) => Db::open(path),
            None => Db::open_default(),
        }
    }

    fn read_provider(&self) -> Option<Provider> {
        let (provider, _) = garmin_coach::load_config(&self.db().ok()?).ok()?;
        provider
    }

    fn write_provider(&self, provider: Provider, model: &str) -> anyhow::Result<()> {
        garmin_coach::save_config(&self.db()?, provider, model)
    }
}

/* ---------------------------------------------------------------- drawing --- */

fn choice(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Div {
    let choices = [
        ("coach-cloud", "Cloud", Provider::Cloud),
        ("coach-key", "Your own key", Provider::Openrouter),
        ("coach-ollama", "Ollama", Provider::Ollama),
    ];
    let mut row = div().flex().items_baseline().gap(px(26.0));
    for (id, label, provider) in choices {
        let on = root.account.cloud.provider == provider;
        row = row.child(
            quiet(id, label, true, false, t)
                .text_color(if on { t.fg } else { t.faint })
                .when(on, |d| d.font_weight(FontWeight::MEDIUM))
                .on_click(
                    cx.listener(move |root, _: &ClickEvent, _, cx| root.cloud_choose(provider, cx)),
                ),
        );
    }
    row
}

fn describe(provider: Provider) -> &'static str {
    match provider {
        Provider::Cloud => {
            "Garmin Companion cloud. It needs no setup. Free use gives 30 credits a day. \
             Only the rows that a question needs leave this computer."
        }
        Provider::Openrouter => {
            "Your own OpenRouter account. OpenRouter bills you. The key stays in the keyring \
             of this computer."
        }
        Provider::Ollama => "A model on this computer. No data leaves it.",
    }
}

fn account_block(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> Div {
    let cloud = &root.account.cloud;
    let mut block = div().mt(px(20.0)).flex().flex_col().gap(px(12.0));
    if let Some(view) = &cloud.view {
        block = block
            .child(body(view.credits_line.clone(), FS_MD, t.fg))
            .child(body(view.plan_line.clone(), FS_SMALL, t.mutd));
    }
    let signed_in = cloud.view.as_ref().is_some_and(|v| v.email.is_some());
    let premium = cloud.view.as_ref().is_some_and(|v| v.premium);
    let mut actions = div().flex().items_baseline().gap(px(26.0));
    actions = if premium {
        actions.child(
            quiet("cloud-portal", "Manage billing", !cloud.busy, false, t)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.cloud_portal(cx))),
        )
    } else {
        actions.child(
            quiet("cloud-premium", "Get premium", !cloud.busy, false, t)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.cloud_checkout(cx))),
        )
    };
    if signed_in {
        actions = actions.child(
            quiet("cloud-sign-out", "Sign out", !cloud.busy, true, t)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.cloud_sign_out(cx))),
        );
    }
    block = block.child(actions);
    if !signed_in {
        block = block.child(sign_in(root, t, window, cx));
    }
    block
}

/// The email step, then the code step. Premium needs it. Free use does not.
fn sign_in(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> Div {
    let cloud = &root.account.cloud;
    let mut col = div()
        .mt(px(14.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .max_w(ch(34.0, FS_SMALL));
    if let Some(email) = &cloud.pending_email {
        col = col
            .child(body(
                format!("Enter the code that went to {email}."),
                FS_SMALL,
                t.mutd,
            ))
            .child(cloud.code.render(
                "cloud-code",
                t,
                cx,
                window,
                |root: &mut Root| &mut root.account.cloud.code,
                |root, _, cx| root.cloud_verify(cx),
            ))
            .child(
                quiet("cloud-verify", "Verify", !cloud.busy, false, t)
                    .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.cloud_verify(cx))),
            );
    } else {
        col = col
            .child(body(
                "Sign in by email to get premium. Free use needs no account.",
                FS_SMALL,
                t.mutd,
            ))
            .child(cloud.email.render(
                "cloud-email",
                t,
                cx,
                window,
                |root: &mut Root| &mut root.account.cloud.email,
                |root, _, cx| root.cloud_send_code(cx),
            ))
            .child(
                quiet("cloud-send", "Send code", !cloud.busy, false, t)
                    .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.cloud_send_code(cx))),
            );
    }
    col
}

fn key_block(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> Div {
    let cloud = &root.account.cloud;
    let state = if cloud.has_key {
        "A key is saved."
    } else {
        "No key is saved yet."
    };
    div()
        .mt(px(20.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .max_w(ch(34.0, FS_SMALL))
        .child(body(state, FS_SMALL, t.mutd))
        .child(cloud.key.render(
            "cloud-key",
            t,
            cx,
            window,
            |root: &mut Root| &mut root.account.cloud.key,
            |root, _, cx| root.cloud_save_key(cx),
        ))
        .child(
            quiet("cloud-save-key", "Save key", cloud.live, false, t)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.cloud_save_key(cx))),
        )
}

/// The Coach section of Settings.
pub fn render(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> Div {
    let cloud = &root.account.cloud;
    let mut block = section(false, "Coach", t)
        .child(prose("Choose where the coach model runs.", t))
        .child(div().mt(px(14.0)).child(choice(root, t, cx)))
        .child(
            body(describe(cloud.provider), FS_SMALL, t.faint)
                .mt(px(10.0))
                .line_height(relative(1.6))
                .max_w(ch(62.0, FS_SMALL)),
        );
    block = match cloud.provider {
        Provider::Cloud => block.child(account_block(root, t, window, cx)),
        Provider::Openrouter => block.child(key_block(root, t, window, cx)),
        Provider::Ollama => block,
    };
    if let Some(note) = &cloud.note {
        let colour = if note.bad { t.warn } else { t.mutd };
        block = block.child(
            body(note.text.clone(), FS_CAPTION, colour)
                .mt(px(14.0))
                .max_w(ch(62.0, FS_CAPTION)),
        );
    }
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_view_has_a_credit_line() {
        let v = sample_account();
        assert_eq!(v.credits_line, "27 of 30 credits left today.");
        assert!(!v.premium && !v.out);
    }

    #[test]
    fn a_job_on_its_own_thread_returns_its_result() {
        let out = pollster_block(on_thread(async { Ok::<_, String>(7) }));
        assert_eq!(out, Ok(7));
        let err = pollster_block(on_thread(async { Err::<u8, _>("no".to_string()) }));
        assert_eq!(err, Err("no".into()));
    }

    /// `async_channel` needs no runtime, so a tiny executor is enough here.
    fn pollster_block<T>(fut: impl Future<Output = T>) -> T {
        use std::task::{Context, Poll, Waker};
        let mut cx = Context::from_waker(Waker::noop());
        let mut fut = Box::pin(fut);
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
