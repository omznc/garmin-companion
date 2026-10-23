//! The update state of the window: the jobs, the banner and the Settings row.
//!
//! The logic is in `updater.rs`. A job runs on its own thread with `on_thread`,
//! and the result comes back to the view. Nothing here blocks the UI thread.

use std::time::Duration;

use crate::scale::px;
use gpui::{div, prelude::*, ClickEvent, Context, Div};

use crate::cloud::on_thread;
use crate::controls::quiet;
use crate::icons::Icon;
use crate::panel::{self, Tone};
use crate::shell::Root;
use crate::theme::{Theme, FS_SMALL};
use crate::ui::body;
use crate::updater::{self, Kind, Offer, Outcome};

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
    pub(crate) fn busy(&self) -> bool {
        self.checking || self.installing
    }
}

impl Root {
    /// Check once after a short delay at each start, then again every six
    /// hours while the app runs. Demo runs never call the network.
    pub fn watch_for_updates(&mut self, cx: &mut Context<Self>) {
        if self.account.demo || !updater::enabled() {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(START_DELAY).await;
            loop {
                // A manual check or an install that runs now gives its own
                // result. A second check would clear its state.
                let Ok(busy) = this.update(cx, |root, _| root.account.update.busy()) else {
                    break;
                };
                if !busy {
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
                Ok(Outcome::Installed) => cx.quit(),
                Ok(Outcome::Cancelled) => {
                    // The user closed the password dialog: keep the offer, show no error.
                    let state = &mut root.account.update;
                    state.installing = false;
                    state.dismissed = false;
                    state.note = None;
                    cx.notify();
                }
                Ok(Outcome::Declined(why)) => {
                    // No install without the system's permission: the user does it by hand.
                    cx.open_url(updater::RELEASE_PAGE);
                    let state = &mut root.account.update;
                    state.installing = false;
                    state.dismissed = false;
                    state.note = Some((
                        format!(
                            "Could not install: {why}. Install the package from the release page."
                        ),
                        true,
                    ));
                    cx.notify();
                }
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

/// The title bar button: its label, and whether a click works. `None` hides it.
/// It shows only an offer that the app can install itself. It hides a manual
/// offer, a check in progress and a demo run.
pub fn title_button(state: &UpdateState, demo: bool) -> Option<(String, bool)> {
    let offer = state.offer.as_ref().filter(|_| !demo && !state.checking)?;
    if offer.kind == Kind::Manual {
        return None;
    }
    Some(if state.installing {
        ("Installing...".into(), false)
    } else {
        (format!("Update to {}", offer.version), true)
    })
}

async fn check_job() -> Result<Option<Offer>, String> {
    updater::check().await.map_err(|e| format!("{e:#}"))
}

fn headline(offer: &Offer) -> String {
    match offer.kind {
        Kind::Manual => format!("Version {} is available.", offer.version),
        Kind::Package(_) => format!(
            "Version {} is ready to install. The system asks for your password.",
            offer.version
        ),
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
        _ if state.installing && matches!(offer.kind, Kind::Package(_)) => {
            "Installing. Answer the password prompt. The app restarts when this is done.".into()
        }
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
            // Under the title bar, over the page.
            .top(px(46.0))
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

/// The status note of the Updates row: a result, an error, or the offer.
fn status(state: &UpdateState, t: &Theme) -> Option<Div> {
    let (text, tone) = match (&state.note, &state.offer) {
        (Some((text, true)), _) => (text.clone(), Tone::Err),
        (Some((text, false)), _) if state.busy() => (text.clone(), Tone::Run),
        (Some((text, false)), _) => (text.clone(), Tone::Ok),
        (None, Some(offer)) => (headline(offer), Tone::Info),
        _ => return None,
    };
    Some(panel::note(tone, text, t))
}

/// The Updates row of the About panel.
pub fn settings_row(root: &Root, t: &Theme, cx: &mut Context<Root>) -> Div {
    let state = &root.account.update;
    let lead = state.checking.then_some(Icon::Spinner);
    let mut buttons = panel::actions().child(
        panel::button(
            "update-check",
            "Check for updates",
            panel::Kind::Plain,
            !state.busy() && !root.account.demo,
            lead,
            t,
        )
        .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.check_for_updates(cx))),
    );
    if let Some(offer) = &state.offer {
        let label = if offer.kind == Kind::Manual {
            "Open the release page"
        } else {
            "Install and restart"
        };
        buttons = buttons.child(
            panel::button(
                "update-install-row",
                label,
                panel::Kind::Primary,
                !state.busy(),
                None,
                t,
            )
            .on_click(cx.listener(|root, _: &ClickEvent, _, cx| root.install_update(cx))),
        );
    }
    panel::row_note(
        "Updates",
        None,
        status(state, t),
        panel::stack().child(buttons),
        t,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(kind: Kind) -> UpdateState {
        UpdateState {
            offer: Some(Offer {
                version: "1.0.9".into(),
                notes: None,
                kind,
                download: None,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn title_button_shows_only_an_installable_offer() {
        let mut s = state(Kind::Windows);
        assert_eq!(
            title_button(&s, false),
            Some(("Update to 1.0.9".into(), true))
        );
        assert_eq!(title_button(&s, true), None);
        s.installing = true;
        assert_eq!(
            title_button(&s, false),
            Some(("Installing...".into(), false))
        );
        s.installing = false;
        s.checking = true;
        assert_eq!(title_button(&s, false), None);
        assert_eq!(title_button(&state(Kind::Manual), false), None);
        assert_eq!(title_button(&UpdateState::default(), false), None);
    }
}
