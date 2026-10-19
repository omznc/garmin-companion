//! The update state of the window: the jobs, the banner and the Settings row.
//!
//! The logic is in `updater.rs`. A job runs on its own thread with `on_thread`,
//! and the result comes back to the view. Nothing here blocks the UI thread.

use std::time::{Duration, SystemTime};

use gpui::{div, prelude::*, px, ClickEvent, Context, Div};

use crate::cloud::on_thread;
use crate::controls::quiet;
use crate::shell::Root;
use crate::theme::{Theme, FS_SMALL};
use crate::ui::{body, SANS};
use crate::updater::{self, Kind, Offer};

/// The wait after the start, so the check does not compete with the first draw.
const START_DELAY: Duration = Duration::from_secs(20);

#[derive(Default)]
pub struct UpdateState {
    pub offer: Option<Offer>,
    /// "Later" hides the banner until the next start. The Settings row stays.
    dismissed: bool,
    checking: bool,
    installing: bool,
    /// A line for Settings: the result of a check, or an error. `true` is an error.
    pub note: Option<(String, bool)>,
}

impl UpdateState {
    fn busy(&self) -> bool {
        self.checking || self.installing
    }
}

impl Root {
    /// Check once after a short delay, then again every six hours while the
    /// app runs. A check that ran less than six hours ago, in any run of the
    /// app, is skipped. Demo runs never call the network.
    pub fn watch_for_updates(&mut self, cx: &mut Context<Self>) {
        if self.account.demo || !updater::enabled() {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(START_DELAY).await;
            loop {
                if updater::due(updater::last_check(), SystemTime::now()) {
                    let result = on_thread(check_job()).await;
                    let alive = this.update(cx, |root, cx| root.on_checked(result, false, cx));
                    if alive.is_err() {
                        break;
                    }
                }
                cx.background_executor().timer(updater::CHECK_EVERY).await;
            }
        })
        .detach();
    }

    /// The "Check for updates" action of Settings.
    pub fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if self.account.demo || self.account.update.busy() {
            return;
        }
        self.account.update.checking = true;
        self.account.update.note = Some(("Checking.".into(), false));
        cx.spawn(async move |this, cx| {
            let result = on_thread(check_job()).await;
            let _ = this.update(cx, |root, cx| root.on_checked(result, true, cx));
        })
        .detach();
        cx.notify();
    }

    fn on_checked(
        &mut self,
        result: Result<Option<Offer>, String>,
        manual: bool,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.account.update;
        state.checking = false;
        match result {
            Ok(offer) => {
                // An offer is not recorded, so a restart shows the banner again.
                if offer.is_none() {
                    updater::record_check();
                }
                state.note = offer
                    .is_none()
                    .then(|| ("This is the latest version.".into(), false));
                if let Some(offer) = offer {
                    // A new version shows the banner again, also after "Later".
                    state.dismissed &= state
                        .offer
                        .as_ref()
                        .is_some_and(|o| o.version == offer.version);
                    state.offer = Some(offer);
                }
            }
            Err(e) => {
                eprintln!("update check failed: {e}");
                // A check in the background stays silent. The user asked for a manual one.
                state.note = manual.then(|| (format!("Could not check for updates: {e}"), true));
            }
        }
        cx.notify();
    }

    /// "Install and restart": download, check the signature, install, quit.
    pub fn install_update(&mut self, cx: &mut Context<Self>) {
        let state = &mut self.account.update;
        let Some(offer) = state.offer.clone().filter(|_| !state.busy()) else {
            return;
        };
        if offer.download.is_none() {
            cx.open_url(updater::RELEASE_PAGE);
            return;
        }
        state.installing = true;
        state.note = Some(("Downloading the update.".into(), false));
        cx.spawn(async move |this, cx| {
            let result =
                on_thread(
                    async move { updater::install(&offer).await.map_err(|e| format!("{e:#}")) },
                )
                .await;
            let _ = this.update(cx, |root, cx| match result {
                Ok(()) => cx.quit(),
                Err(e) => {
                    let state = &mut root.account.update;
                    state.installing = false;
                    state.dismissed = false;
                    state.note = Some((format!("The update failed: {e}"), true));
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn dismiss_update(&mut self, cx: &mut Context<Self>) {
        self.account.update.dismissed = true;
        cx.notify();
    }
}

async fn check_job() -> Result<Option<Offer>, String> {
    updater::check().await.map_err(|e| format!("{e:#}"))
}

fn headline(offer: &Offer) -> String {
    match offer.kind {
        Kind::Manual => format!("Version {} is available.", offer.version),
        _ => format!("Version {} is ready to install.", offer.version),
    }
}

/// The banner, at the bottom of the window. `None` when there is nothing to show.
pub fn banner(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Option<Div> {
    let state = &root.account.update;
    let offer = state.offer.as_ref().filter(|_| !state.dismissed)?;
    let manual = offer.kind == Kind::Manual;
    let line = match &state.note {
        Some((text, true)) => text.clone(),
        _ if state.installing => "Installing. The app restarts when this is done.".into(),
        _ => headline(offer),
    };
    let act = if manual {
        "Open the release page"
    } else {
        "Install and restart"
    };
    let actions = div()
        .flex()
        .items_baseline()
        .gap(px(22.0))
        .child(
            quiet("update-install", act, !state.busy(), false, t)
                .text_color(t.acc)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.install_update(cx))),
        )
        .child(
            quiet("update-later", "Later", !state.installing, false, t)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.dismiss_update(cx))),
        );
    Some(
        div()
            .absolute()
            // Under the title strip, clear of the Ask composer at the bottom.
            .top(px(56.0))
            .right(px(24.0))
            .max_w(px(340.0))
            .px(px(18.0))
            .py(px(14.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(t.line)
            .bg(t.bg2)
            .shadow_md()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(body(
                line,
                FS_SMALL,
                if state.note.as_ref().is_some_and(|n| n.1) {
                    t.warn
                } else {
                    t.fg
                },
            ))
            .child(actions),
    )
}

/// A row for the About section of Settings.
pub fn settings_row(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Div {
    let state = &root.account.update;
    let status = match (&state.note, &state.offer) {
        (Some((text, bad)), _) => Some((text.clone(), *bad)),
        (None, Some(offer)) => Some((headline(offer), false)),
        _ => None,
    };
    let mut row = div()
        .mt(px(12.0))
        .flex()
        .items_baseline()
        .gap(px(22.0))
        .font_family(SANS)
        .child(
            quiet(
                "update-check",
                "Check for updates",
                !state.busy() && !root.account.demo,
                false,
                t,
            )
            .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.check_for_updates(cx))),
        );
    if state.offer.is_some() {
        row = row.child(
            quiet("update-install-row", "Install", !state.busy(), false, t)
                .text_color(t.acc)
                .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.install_update(cx))),
        );
    }
    if let Some((text, bad)) = status {
        row = row.child(body(text, FS_SMALL, if bad { t.warn } else { t.mutd }));
    }
    row
}
