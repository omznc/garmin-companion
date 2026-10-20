//! Phosphor icons, compiled into the binary.
//!
//! gpui keeps only the alpha mask of an SVG and tints it with one colour. A
//! Phosphor duotone icon is a shape at 20% opacity under the same shape's
//! strokes. The current app paints that back layer with the accent, so one tint
//! is not enough. [`Icons`] therefore answers for two virtual files per
//! duotone icon, `<name>.back.svg` and `<name>.front.svg`. [`icon`] draws the
//! two as two stacked `svg()` elements with two colours.

use std::borrow::Cow;

use gpui::{div, prelude::*, px, svg, AssetSource, Hsla, SharedString};

use crate::theme::Theme;

macro_rules! icons {
    ($($variant:ident => $file:literal,)*) => {
        // The table lists icons for screens that come in a later phase.
        #[allow(dead_code)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Icon {
            $($variant,)*
        }

        impl Icon {
            /// The path of the file, without the layer infix.
            fn stem(self) -> &'static str {
                match self {
                    $(Icon::$variant => $file,)*
                }
            }

            #[cfg(test)]
            const ALL: &'static [Icon] = &[$(Icon::$variant,)*];
        }

        const FILES: &[(&str, &str)] = &[
            $(($file, include_str!(concat!("../assets/icons/", $file, ".svg"))),)*
        ];
    };
}

icons! {
    // The window controls are strokes, so they take the bold weight.
    // Duotone fills the shape behind a stroke, and a minus has none.
    Minimise => "bold/minus",
    Maximise => "bold/square",
    Restore => "bold/copy-simple",
    Close => "bold/x",

    Sync => "duotone/arrows-clockwise",
    ArrowRight => "duotone/arrow-right",
    Spinner => "duotone/circle-notch",
    Light => "duotone/sun",
    Dark => "duotone/moon",
    SignOut => "duotone/sign-out",
    Check => "duotone/check",
    Warning => "duotone/warning",
    Info => "duotone/info",
    Desktop => "duotone/desktop",
    Search => "duotone/magnifying-glass",

    NavToday => "duotone/house",
    NavActivities => "duotone/sneaker-move",
    NavHealth => "duotone/heartbeat",
    NavSleep => "duotone/moon-stars",
    NavFood => "duotone/fork-knife",
    NavWeight => "duotone/scales",
    NavAsk => "duotone/chat-circle-dots",
    NavInsights => "duotone/lightbulb",
    NavPlan => "duotone/calendar-blank",
    NavRoutes => "duotone/map-trifold",
    NavGear => "duotone/backpack",
    NavStrength => "duotone/barbell",
    NavFitness => "duotone/gauge",
    NavReports => "duotone/file-text",
    NavSettings => "duotone/gear-six",

    Send => "bold/arrow-up",
    Stop => "bold/stop",
}

impl Icon {
    fn is_duotone(self) -> bool {
        self.stem().starts_with("duotone/")
    }

    fn path(self, layer: &str) -> SharedString {
        format!("icons/{}.{layer}.svg", self.stem()).into()
    }
}

/// One icon at a size. Duotone icons get the accent tint behind the strokes.
pub fn icon(which: Icon, size: f32, theme: &Theme, colour: Hsla) -> gpui::Div {
    icon_hover(which, size, theme, colour, None)
}

/// An icon whose strokes change colour while the pointer is over a group.
///
/// An icon draws in its own colour. It does not inherit the text colour of the
/// element around it, so a hover rule on the parent cannot reach it.
pub fn icon_hover(
    which: Icon,
    size: f32,
    theme: &Theme,
    colour: Hsla,
    hover: Option<(&'static str, Hsla)>,
) -> gpui::Div {
    let layer = |path: SharedString, colour: Hsla| {
        svg()
            .path(path)
            .size(px(size))
            .flex_none()
            .text_color(colour)
            .absolute()
            .top_0()
            .left_0()
    };
    let mut stack = div().relative().size(px(size)).flex_none();
    if which.is_duotone() {
        stack = stack.child(layer(which.path("back"), theme.icon2));
    }
    let front = layer(which.path("front"), colour);
    stack.child(match hover {
        Some((group, on)) => front.group_hover(group, move |s| s.text_color(on)),
        None => front,
    })
}

/// Split one Phosphor file into its back and front layers.
///
/// Phosphor marks the back layer with `opacity="0.2"`. The back copy drops the
/// attribute, because the tint colour already carries the alpha.
fn layer(svg_text: &str, want_back: bool) -> String {
    let head_end = svg_text.find("><path").map_or(0, |i| i + 1);
    let (head, body) = svg_text.split_at(head_end);
    let body = body.trim_end().trim_end_matches("</svg>");
    let mut out = String::from(head);
    for piece in body.split("<path").filter(|p| !p.trim().is_empty()) {
        let is_back = piece.contains("opacity=\"0.2\"");
        if is_back == want_back {
            out.push_str("<path");
            out.push_str(&piece.replace(" opacity=\"0.2\"", ""));
        }
    }
    out.push_str("</svg>");
    out
}

/// The icons as gpui asks for them. Hand this to `Application::with_assets`.
pub struct Icons;

impl AssetSource for Icons {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let Some(rest) = path.strip_prefix("icons/") else {
            return Ok(None);
        };
        let Some((stem, layer_name)) = rest
            .strip_suffix(".svg")
            .and_then(|r| r.rsplit_once('.'))
            .filter(|(_, l)| matches!(*l, "back" | "front"))
        else {
            return Ok(None);
        };
        let Some((_, text)) = FILES.iter().find(|(name, _)| *name == stem) else {
            return Ok(None);
        };
        Ok(Some(Cow::Owned(
            layer(text, layer_name == "back").into_bytes(),
        )))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(FILES
            .iter()
            .map(|(name, _)| format!("icons/{name}.front.svg"))
            .filter(|p| p.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_has_a_front_layer_and_every_duotone_icon_a_back_layer() {
        for &which in Icon::ALL {
            let front = Icons
                .load(&which.path("front"))
                .unwrap()
                .expect("front layer");
            assert!(
                String::from_utf8_lossy(&front).contains("<path"),
                "{which:?}"
            );
            let back = Icons
                .load(&which.path("back"))
                .unwrap()
                .expect("back layer");
            let has_path = String::from_utf8_lossy(&back).contains("<path");
            assert_eq!(has_path, which.is_duotone(), "{which:?}");
        }
    }

    #[test]
    fn the_back_layer_drops_the_opacity_the_tint_carries() {
        let text = include_str!("../assets/icons/duotone/house.svg");
        let back = layer(text, true);
        let front = layer(text, false);
        assert!(!back.contains("opacity") && back.matches("<path").count() == 1);
        assert!(!front.contains("opacity") && front.matches("<path").count() == 1);
    }

    #[test]
    fn an_unknown_path_is_not_an_error() {
        assert!(Icons.load("icons/nope/none.front.svg").unwrap().is_none());
        assert!(Icons.load("fonts/x.ttf").unwrap().is_none());
    }
}
