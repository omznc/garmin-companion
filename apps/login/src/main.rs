//! The Garmin sign-in window.
//!
//! gpui has no browser view, and Garmin's sign-in needs a real browser: the
//! page sits behind a Cloudflare check on the TLS handshake. So the desktop app
//! starts this small program. It shows Garmin's own page, so the app never
//! sees the password. When Garmin redirects to the service with a ticket, the
//! program stops the redirect, prints the ticket on one line of stdout and
//! exits with code 0. The app trades the ticket for tokens.
//!
//! Usage: `garmin-companion-login <sign-in url>`. The URL must be https on
//! garmin.com. Exit code 1 means that no ticket came: the user closed the
//! window, or the wait ran out. Exit code 2 means bad arguments.

// No console window on Windows in a release build.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::process::exit;
use std::time::{Duration, Instant};

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

/// Long enough for a slow sign-in with two-factor. Short enough that a window
/// left open does not wait for ever.
const TIMEOUT: Duration = Duration::from_secs(600);

/// An https URL on garmin.com or one of its subdomains.
fn is_garmin_https(link: &str) -> bool {
    let Ok(url) = url::Url::parse(link) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default();
    url.scheme() == "https" && (host == "garmin.com" || host.ends_with(".garmin.com"))
}

/// Linux: the WebKitGTK DMA-BUF renderer fails to allocate buffers on NVIDIA
/// with Wayland. The web process then dies and the window stays blank. Set the
/// variable before WebKit starts. A value that the user already set wins.
#[cfg(target_os = "linux")]
fn work_around_webkit_bugs() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: this runs first in `main`, before any thread starts.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
}

/// macOS: a window has no Edit menu by default, so the copy, paste and select
/// all shortcuts do nothing. Add the standard items. The first submenu is the
/// app menu. It holds only "Close Window", because "Quit" would exit with code
/// 0 and no ticket.
#[cfg(target_os = "macos")]
fn install_menu() {
    use muda::{Menu, PredefinedMenuItem, Submenu};

    let menu = Menu::new();
    let app = Submenu::with_items("", true, &[&PredefinedMenuItem::close_window(None)]);
    let edit = Submenu::with_items(
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::select_all(None),
        ],
    );
    if let (Ok(app), Ok(edit)) = (app, edit) {
        if menu.append(&app).is_ok() && menu.append(&edit).is_ok() {
            menu.init_for_nsapp();
        }
    }
    // The menu must live as long as the process.
    std::mem::forget(menu);
}

fn main() -> wry::Result<()> {
    let Some(start) = std::env::args().nth(1).filter(|u| is_garmin_https(u)) else {
        eprintln!("usage: garmin-companion-login <https sign-in url on garmin.com>");
        exit(2);
    };
    #[cfg(target_os = "linux")]
    work_around_webkit_bugs();

    let event_loop = EventLoopBuilder::<String>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title("Sign in to Garmin")
        .with_inner_size(tao::dpi::LogicalSize::new(520.0, 760.0))
        .build(&event_loop)
        .expect("could not open the sign-in window");
    #[cfg(target_os = "macos")]
    install_menu();

    let builder = WebViewBuilder::new()
        .with_url(&start)
        .with_navigation_handler(move |link| match garmin_api::session::ticket_in(&link) {
            Some(ticket) => {
                // Stop the redirect. The ticket works once, and Garmin Connect
                // would spend it before the app can.
                let _ = proxy.send_event(ticket);
                false
            }
            None => true,
        });

    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    ))]
    let _webview = builder.build(&window)?;
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    )))]
    let _webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window.default_vbox().expect("the window has no GTK box");
        builder.build_gtk(vbox)?
    };

    let deadline = Instant::now() + TIMEOUT;
    event_loop.run(move |event, _, control_flow| {
        // Keep the window and the view alive for the whole run.
        let _ = (&window, &_webview);
        *control_flow = ControlFlow::WaitUntil(deadline);
        match event {
            Event::UserEvent(ticket) => {
                println!("{ticket}");
                exit(0);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => exit(1),
            _ if Instant::now() >= deadline => exit(1),
            _ => {}
        }
    })
}

#[cfg(test)]
mod tests {
    use super::is_garmin_https;

    #[test]
    fn only_https_on_garmin_is_allowed() {
        assert!(is_garmin_https(
            "https://sso.garmin.com/portal/sso/en-US/sign-in?x=1"
        ));
        assert!(!is_garmin_https("http://sso.garmin.com/"));
        assert!(!is_garmin_https("https://notgarmin.com/"));
        assert!(!is_garmin_https("https://garmin.com.evil.test/"));
        assert!(!is_garmin_https("file:///etc/passwd"));
        assert!(!is_garmin_https("nope"));
    }
}
