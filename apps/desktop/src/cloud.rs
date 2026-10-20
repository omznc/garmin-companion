//! The coach settings: where the model runs, the cloud account, the sign-in.
//!
//! The state lives in `Account::cloud`. A call to the worker runs on its own
//! thread with a small tokio runtime, like the Ask turn does, and the result
//! comes back to the view through a channel. The view never sees a token: the
//! core sends an `AccountView` with the sentences already built.
//!
//! In a demo (`--persona` or `--db`) nothing leaves the computer: the account
//! section shows a note, and the actions say that they are off.

use std::future::Future;
use std::time::Duration;

use chrono::Utc;
use garmin_coach::{AccountView, Balance, CloudClient, Provider};
use garmin_core::{db::Db, store};
use gpui::{div, prelude::*, px, App, ClickEvent, Context, Div, FontWeight, Window};

use crate::data::Source;
use crate::field::Field;
use crate::panel::{self, Kind, Tone};
use crate::shell::Root;
use crate::theme::{Theme, FS_BASE};
use crate::ui::body;

/// The checkout page is watched for 2 minutes: every 3 seconds for the first
/// 30 seconds, when a payment is most likely, then every 15 seconds.
const POLL_FAST: usize = 10;
const POLL_TIMES: usize = 16;

fn poll_delay(round: usize) -> Duration {
    Duration::from_secs(if round < POLL_FAST { 3 } else { 15 })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub text: String,
    pub bad: bool,
}

pub struct Cloud {
    /// False in a demo (`--persona` or `--db`): no network, no secret store, no
    /// database write.
    pub live: bool,
    pub provider: Provider,
    pub view: Option<AccountView>,
    pub note: Option<Note>,
    /// The email that a code went to. `None` while no code is waiting.
    pending_email: Option<String>,
    busy: bool,
    /// The running checkout poll. A new one replaces it: dropping a task stops it.
    watch: Option<gpui::Task<()>>,
    /// The poll has not ended. A window that comes back reads the balance once.
    watching: bool,
    has_key: bool,
    email: Field,
    code: Field,
    key: Field,
}

impl Cloud {
    pub fn new(cx: &mut App) -> Self {
        Self {
            live: false,
            provider: Provider::Cloud,
            view: None,
            note: None,
            pending_email: None,
            busy: false,
            watch: None,
            watching: false,
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

/// The balance if this install exists, else `None`. A background read must not
/// enrol: the worker caps new installs each day.
pub async fn account_quiet() -> Result<Option<AccountView>, String> {
    text(CloudClient::default().balance_if_enrolled().await)
        .map(|b| b.map(|b: Balance| b.view(Utc::now())))
}

/// The model to save when the person picks `provider`. An empty name keeps the
/// saved one. A model from another provider means nothing to OpenRouter, so
/// switching to "Your own key" starts from the default model.
fn model_for(provider: Provider, previous: Provider) -> &'static str {
    if provider == Provider::Openrouter && previous != Provider::Openrouter {
        garmin_coach::DEFAULT_OPENROUTER_MODEL
    } else {
        ""
    }
}

/// Only the default source may reach the network, the keyring or the saved
/// choice. A persona and a database copy are demos.
fn is_live(source: &Source) -> bool {
    matches!(source, Source::Default)
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
        let live = is_live(&self.source);
        let cloud = &mut self.account.cloud;
        cloud.live = live;
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
        let model = model_for(provider, cloud.provider);
        self.cloud_keep(provider, model, cx);
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
    /// `from_ask` is set when the Ask screen started it. The notes show only on
    /// Settings, so a failure there moves the person to Settings to read it.
    pub fn cloud_checkout(&mut self, from_ask: bool, cx: &mut Context<Self>) {
        let cloud = &mut self.account.cloud;
        let refused = if !cloud.live {
            Some("Checkout is off in a demo.")
        } else if cloud.view.as_ref().is_some_and(|v| v.email.is_none()) {
            Some("Sign in with your email first. Premium belongs to an account.")
        } else {
            None
        };
        if let Some(why) = refused {
            cloud.say(why, true);
            return self.show_settings_if(from_ask, cx);
        }
        let job = async { text(CloudClient::default().checkout_url().await) };
        self.cloud_call(cx, job, move |root, out, cx| match out {
            Ok(url) => {
                cx.open_url(&url);
                root.account.cloud.say(
                    "The checkout opened in your browser. This page updates when the payment is done.",
                    false,
                );
                root.cloud_watch(cx);
            }
            Err(e) => {
                root.account.cloud.say(e, true);
                root.show_settings_if(from_ask, cx);
            }
        });
    }

    fn show_settings_if(&mut self, yes: bool, cx: &mut Context<Self>) {
        if yes {
            self.show_settings(cx);
        }
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

    /// Read the balance for 2 minutes, until premium shows. See `poll_delay`.
    fn cloud_watch(&mut self, cx: &mut Context<Self>) {
        self.account.cloud.watching = true;
        self.account.cloud.watch = Some(cx.spawn(async move |this, cx| {
            for round in 0..POLL_TIMES {
                cx.background_executor().timer(poll_delay(round)).await;
                if poll_once(&this, cx).await {
                    return;
                }
            }
            let _ = this.update(cx, |root, cx| {
                let cloud = &mut root.account.cloud;
                cloud.watching = false;
                cloud.say("Still waiting. Open Settings to check.", false);
                cx.notify();
            });
        }));
    }

    /// The window came back: the payment may be done, so read the balance now.
    pub fn on_activated(&mut self, cx: &mut Context<Self>) {
        if !self.account.cloud.watching {
            return;
        }
        cx.spawn(async move |this, cx| {
            poll_once(&this, cx).await;
        })
        .detach();
    }
}

/// Read the balance once for the checkout watch. True when the watch is over:
/// premium shows, or the window is gone.
async fn poll_once(this: &gpui::WeakEntity<Root>, cx: &mut gpui::AsyncApp) -> bool {
    let Ok(view) = on_thread(account()).await else {
        return false;
    };
    let premium = view.premium;
    let alive = this.update(cx, |root, cx| {
        if premium {
            let cloud = &mut root.account.cloud;
            cloud.watching = false;
            cloud.say("Premium is active. Thank you.", false);
            // The Ask screen shows its own copy of the balance.
            root.ask.update(cx, |ask, cx| ask.refresh_credits(cx));
        }
        root.account.cloud.view = Some(view);
        cx.notify();
    });
    alive.is_err() || premium
}

impl Cloud {
    fn db(&self) -> anyhow::Result<Db> {
        Db::open_default()
    }

    fn read_provider(&self) -> Option<Provider> {
        let (provider, _) = garmin_coach::load_config(&self.db().ok()?).ok()?;
        provider
    }

    /// An empty `model` keeps the model that is saved.
    fn write_provider(&self, provider: Provider, model: &str) -> anyhow::Result<()> {
        let db = self.db()?;
        let kept = garmin_coach::load_config(&db)?.1.unwrap_or_default();
        let model = if model.is_empty() { &kept } else { model };
        garmin_coach::save_config(&db, provider, model)
    }
}

/* ---------------------------------------------------------------- drawing --- */

/// The three cards: the name, a short tag and what the choice means.
const CHOICES: [(&str, &str, &str, &str, Provider); 3] = [
    (
        "coach-cloud",
        "Cloud",
        "Free credits",
        "Only the rows that a question needs leave this computer.",
        Provider::Cloud,
    ),
    (
        "coach-key",
        "Your own key",
        "OpenRouter",
        "Questions go to OpenRouter with your key. OpenRouter bills you.",
        Provider::Openrouter,
    ),
    (
        "coach-ollama",
        "Ollama",
        "Local",
        "A model on this computer. No data leaves it.",
        Provider::Ollama,
    ),
];

fn choice(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Div {
    let mut cards = panel::opts();
    for (id, name, tag, desc, provider) in CHOICES {
        let on = root.account.cloud.provider == provider;
        cards = cards.child(panel::opt(id, name, Some(tag), desc, on, t).on_click(
            cx.listener(move |root, _: &ClickEvent, _, cx| root.cloud_choose(provider, cx)),
        ));
    }
    panel::padded(div()).child(cards)
}

/// The cloud account section of a demo. No sign-in form: nothing is sent.
const DEMO_NOTE: &str = "The cloud account is off in a demo. Nothing is sent.";

/// A bold label over a help line, above a form.
fn form_head(label: &str, help: &str, t: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .child(body(label.to_string(), 14.5, t.fg).font_weight(FontWeight::MEDIUM))
        .child(panel::help(help.to_string(), t))
}

/// The credits line over the plan line. The account view has no daily total,
/// so there is no meter: a bar would need a number that is not there.
fn credits(view: &AccountView, t: &Theme) -> Div {
    div()
        .flex()
        .flex_wrap()
        .justify_between()
        .items_baseline()
        .gap_x(px(16.0))
        .gap_y(px(6.0))
        .child(body(view.credits_line.clone(), FS_BASE, t.fg).font_weight(FontWeight::MEDIUM))
        .child(panel::help(view.plan_line.clone(), t))
}

fn account_block(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> Div {
    let cloud = &root.account.cloud;
    let mut block = div().flex().flex_col().gap(px(16.0));
    if let Some(view) = &cloud.view {
        block = block.child(credits(view, t));
    }
    if !cloud.live {
        return block.child(panel::note(Tone::Info, DEMO_NOTE, t));
    }
    match cloud.view.as_ref().and_then(|v| v.email.clone()) {
        Some(email) => block.child(signed_in_part(root, &email, t, cx)),
        None => block.child(sign_in(root, t, window, cx)),
    }
}

/// Premium or billing, and the sign-out.
fn signed_in_part(root: &Root, email: &str, t: &Theme, cx: &mut Context<Root>) -> Div {
    let cloud = &root.account.cloud;
    let premium = cloud.view.as_ref().is_some_and(|v| v.premium);
    let main = if premium {
        panel::button(
            "cloud-portal",
            "Manage billing",
            Kind::Plain,
            !cloud.busy,
            None,
            t,
        )
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.cloud_portal(cx)))
    } else {
        panel::button(
            "cloud-premium",
            "Get premium",
            Kind::Primary,
            !cloud.busy,
            None,
            t,
        )
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.cloud_checkout(false, cx)))
    };
    let out = panel::button(
        "cloud-sign-out",
        "Sign out",
        Kind::Danger,
        !cloud.busy,
        None,
        t,
    )
    .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.cloud_sign_out(cx)));
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(body(format!("Signed in as {email}."), 14.5, t.fg))
        .child(panel::actions().child(main).child(out))
}

/// The email step, then the code step. Premium needs it. Free use does not.
fn sign_in(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> Div {
    let cloud = &root.account.cloud;
    let head = form_head(
        "Account",
        "Sign in by email to get premium. Free use needs no account.",
        t,
    );
    let email = panel::field_line()
        .child(panel::input_box(cloud.email.render(
            "cloud-email",
            t,
            cx,
            window,
            |root: &mut Root| &mut root.account.cloud.email,
            |root, _, cx| root.cloud_send_code(cx),
        )))
        .child(
            panel::button("cloud-send", "Send code", Kind::Plain, !cloud.busy, None, t)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.cloud_send_code(cx))),
        );
    let mut col = div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(head)
        .child(email);
    if let Some(sent) = &cloud.pending_email {
        let code = panel::field_line()
            .child(panel::input_box(cloud.code.render(
                "cloud-code",
                t,
                cx,
                window,
                |root: &mut Root| &mut root.account.cloud.code,
                |root, _, cx| root.cloud_verify(cx),
            )))
            .child(
                panel::button(
                    "cloud-verify",
                    "Verify",
                    Kind::Primary,
                    !cloud.busy,
                    None,
                    t,
                )
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.cloud_verify(cx))),
            );
        col = col
            .child(panel::help(
                format!("Enter the code that went to {sent}."),
                t,
            ))
            .child(code);
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
    let label = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_x(px(12.0))
        .gap_y(px(8.0))
        .child(body("OpenRouter API key", 14.5, t.fg).font_weight(FontWeight::MEDIUM))
        .child(panel::chip(state, cloud.has_key, t));
    let form = panel::field_line()
        .child(panel::input_box(cloud.key.render(
            "cloud-key",
            t,
            cx,
            window,
            |root: &mut Root| &mut root.account.cloud.key,
            |root, _, cx| root.cloud_save_key(cx),
        )))
        .child(
            panel::button(
                "cloud-save-key",
                "Save key",
                Kind::Plain,
                cloud.live,
                None,
                t,
            )
            .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.cloud_save_key(cx))),
        );
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(label)
        .child(form)
        .child(panel::help(
            "The key stays in the keyring of this computer. OpenRouter bills you.",
            t,
        ))
}

/// Ollama has no setting here. A failed probe shows as the note below.
fn ollama_block(t: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(body("Ollama", 14.5, t.fg).font_weight(FontWeight::MEDIUM))
        .child(panel::help(
            "A model on this computer. No data leaves it.",
            t,
        ))
}

/// The area under the cards: the part of the chosen provider, then the last note.
fn detail(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> Div {
    let cloud = &root.account.cloud;
    let part = match cloud.provider {
        Provider::Cloud => account_block(root, t, window, cx),
        Provider::Openrouter => key_block(root, t, window, cx),
        Provider::Ollama => ollama_block(t),
    };
    let mut area = panel::sub(t).child(part);
    if let Some(n) = &cloud.note {
        let tone = if n.bad { Tone::Err } else { Tone::Ok };
        area = area.child(panel::note(tone, n.text.clone(), t));
    }
    area
}

/// The Coach section of Settings.
pub fn render(root: &Root, t: &Theme, window: &Window, cx: &mut Context<Root>) -> Div {
    let rows = panel::panel(t)
        .child(choice(root, t, cx))
        .child(detail(root, t, window, cx));
    panel::section("Coach", Some("Choose where the coach model runs."), t).child(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_default_source_is_live() {
        assert!(is_live(&Source::Default));
        assert!(!is_live(&Source::Persona("sample".into())));
        let copy = Source::Db {
            path: "copy.db".into(),
            date: None,
        };
        assert!(!is_live(&copy));
    }

    #[test]
    fn the_sample_view_has_a_credit_line() {
        let v = sample_account();
        assert_eq!(v.credits_line, "27 of 30 credits left today.");
        assert!(!v.premium && !v.out);
    }

    #[test]
    fn the_checkout_watch_polls_less_often_after_30_seconds() {
        let total: u64 = (0..POLL_TIMES).map(|i| poll_delay(i).as_secs()).sum();
        assert_eq!(total, 120);
        assert!(poll_delay(0) < poll_delay(POLL_FAST));
        let first: u64 = (0..POLL_FAST).map(|i| poll_delay(i).as_secs()).sum();
        assert_eq!(first, 30);
    }

    #[test]
    fn a_key_starts_from_the_default_model_after_another_provider() {
        let default = garmin_coach::DEFAULT_OPENROUTER_MODEL;
        assert_eq!(model_for(Provider::Openrouter, Provider::Ollama), default);
        assert_eq!(model_for(Provider::Openrouter, Provider::Cloud), default);
        assert_eq!(model_for(Provider::Openrouter, Provider::Openrouter), "");
        assert_eq!(model_for(Provider::Ollama, Provider::Openrouter), "");
        assert_eq!(model_for(Provider::Cloud, Provider::Ollama), "");
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
